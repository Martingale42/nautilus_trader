# Sinopac 模擬停損/條件單 + 能力文件稽核 — 設計 Spec

- **日期**: 2026-06-26
- **分支**: `sinopac-adapter-clean`
- **狀態**: Draft（待 user review）
- **範圍**: #1 停損/條件單（Standard）+ #3 整合文件完整能力稽核
- **相關元件**: `nautilus_trader/adapters/sinopac/execution.py`、`nautilus_trader/execution/emulator.pyx`、`docs/integrations/sinopac.md`、`examples/live/sinopac/sinopac_exec_tester.py`

---

## 1. 背景與問題陳述

Sinopac adapter 目前的 order type 只支援 `MARKET` / `LIMIT` / `MARKET_TO_LIMIT`（MKP）。
策略常用的風控單型 —— `STOP_MARKET`、`STOP_LIMIT`、`TRAILING_STOP_*`、
`MARKET_IF_TOUCHED`、`LIMIT_IF_TOUCHED` —— 完全無法使用。

**關鍵事實：Sinopac / Shioaji 沒有原生停損/觸價單。** Shioaji 的 `api.Order`
只有 `price_type`（LMT/MKT/MKP）與 `order_type`（ROD/IOC/FOK），**沒有
trigger price 欄位**。官方文件的「觸價委託」是客戶端自己訂閱 tick、自己盯價，
價格到了才呼叫 `place_order` 送一張普通 LMT/MKT 單。因此「把停損單送給交易所」
在此 venue 不可行 —— 觸發邏輯一定要 host 在某個會盯行情的地方。

**現存缺陷（本案一併修正）**：`execution.py:944-946` 對任何不在
`_NT_TO_SINOPAC_PRICE_TYPE`（`execution.py:169-173`，僅含 LIMIT/MARKET/
MARKET_TO_LIMIT）的單型，只 `self._log.error(...)` 後 `return`，**不發出任何
`OrderRejected` 事件**。結果：策略送出裸停損單會被「靜默丟棄」，NT 端訂單永遠
卡在未終結狀態。

---

## 2. 決策：採用 NautilusTrader `OrderEmulator`（in-process）

NT 內建 `OrderEmulator`（`nautilus_trader/execution/emulator.pyx`，`Actor`）專為
「交易所不支援進階單型」設計，坐在策略與 execution client 之間：

1. 策略送 `StopMarketOrder`/`StopLimitOrder`/`TrailingStop*` 並帶
   `emulation_trigger`（`LAST_PRICE` 或 `BID_ASK`）。
2. emulator 攔截（`emulator.pyx:356` `_handle_submit_order`），本地建
   `MatchingCore`，並**自動向 data client 訂閱所需行情**：
   `LAST_PRICE` → trade ticks；`BID_ASK`/`DEFAULT` → quote ticks + order book
   deltas（`emulator.pyx:403-412`）。Sinopac data client 這兩種行情皆已串流。
3. 單子被 hold（狀態 `EMULATED`），每筆行情用 `match_order` 檢查觸發。
4. 觸發後轉成普通 `MARKET`/`LIMIT` 單，走正常路徑送進 Sinopac execution client
   （已支援）。**broker 永遠只看到普通單。**

`SUPPORTED_TRIGGERS = {DEFAULT, BID_ASK, LAST_PRICE}`（`emulator.pyx:78`）。
emulator 亦原生支援 `TRAILING_STOP_MARKET`/`TRAILING_STOP_LIMIT`
（`emulator.pyx:393`）與 OTO/OCO bracket 單（`emulator.pyx:448`）。

### 已排除的替代方案
- **B：gateway 常駐觸發引擎** — 把觸價引擎放進 `shioaji-server`。優點是 NT 行程
  當掉仍能觸發；缺點是跨 repo、gateway 需持有行情狀態 + 持久化 + 斷線重放，
  複雜度與測試成本高很多。**列為未來 resilience backlog，不在本案。**
- **C：adapter 自建觸發引擎** — 在 exec client 內自建 pending-stop 管理器盯 WS
  tick 流，等於重造 `OrderEmulator`，程式碼與風險高、非慣用做法。**否決。**

---

## 3. Part A — 條件單設計（#1）

### 3.1 資料流（總覽）

```
Strategy
  └─ submit StopMarketOrder(emulation_trigger=LAST_PRICE)
       └─ OrderEmulator  ──(hold + 訂 trade ticks)──┐
                                                     │ 盯價
            觸發 ◀────────── Sinopac trade tick ─────┘
              └─ release MARKET order
                   └─ SinopacExecutionClient._submit_order  ← 已支援
                        └─ gateway POST /orders/place
```

裸停損（未帶 `emulation_trigger`，即 `NO_TRIGGER`）不經過 emulator，直接到
`_submit_order`，必須被乾淨拒絕。

### 3.2 程式碼變更（adapter 端，範圍小而明確）

**變更 1（核心）：修正 `_submit_order` 的靜默丟棄。**
`execution.py:944-946` 改為發出 `generate_order_rejected(...)`：
- 對「已知條件單型」（`STOP_MARKET`、`STOP_LIMIT`、`MARKET_IF_TOUCHED`、
  `LIMIT_IF_TOUCHED`、`TRAILING_STOP_MARKET`、`TRAILING_STOP_LIMIT`）給**專屬
  訊息**（English log/exception 規範）：
  `"Sinopac has no native stop/conditional orders; resubmit with emulation_trigger=LAST_PRICE or BID_ASK"`
- 對真正未知/不支援的型別給泛用拒絕訊息：
  `"Unsupported order type {order_type} for Sinopac"`
- 兩者都呼叫 `generate_order_rejected`，使訂單正確進入 `REJECTED` 終結狀態。

實作細節：在現有 `if order.order_type not in _NT_TO_SINOPAC_PRICE_TYPE:` 分支內，
以一個 conditional-order-type 集合區分訊息；之後 `generate_order_rejected(...)`
取代 `return`。注意此檢查目前在 `generate_order_submitted` **之前**，需確認拒絕
事件不依賴先發 submitted（NT 允許 INITIALIZED → REJECTED）。

**變更 2：裸 bracket 子單一併覆蓋。**
`_submit_order_list`（`execution.py:1186-1195`）逐筆呼叫 `_submit_order`，故
bracket 內的裸停損子單走同一條拒絕路徑，無需額外程式碼。

**變更 3：release 路徑 — 不需改，但需驗證。**
觸發後 emulator release 的是普通 `MARKET`/`LIMIT` 單，既有路徑可送出。需驗證
（見 §3.5）：(a) tags 在 emulation 後仍保留；(b) TIF coercion 對 release 單正確。

### 3.3 Order type → 行為對照

| NT 單型 | 帶 emulation_trigger | 行為 |
|---|---|---|
| `STOP_MARKET` | 是 | emulator hold → 觸發 release `MARKET` → adapter 送出（TIF 自動→IOC） |
| `STOP_LIMIT` | 是 | emulator hold → 觸發 release `LIMIT` → adapter 送出（DAY→ROD） |
| `TRAILING_STOP_MARKET/LIMIT` | 是 | 同上，emulator 處理 trailing |
| `MARKET_IF_TOUCHED`/`LIMIT_IF_TOUCHED` | 是 | 同 STOP_MARKET/STOP_LIMIT |
| 上述任一 | 否（NO_TRIGGER） | adapter 發 `OrderRejected` 帶引導訊息 |
| `MARKET`/`LIMIT`/`MARKET_TO_LIMIT` | n/a | 既有行為不變 |

### 3.4 觸發後 TIF / 價格 coercion（既有，確認可用）

- `STOP_MARKET` → `MARKET`，經 `_resolve_order_type`（`execution.py:185-228`）：
  market 單 + DAY/GTC → 強制 IOC（warning）。
- `STOP_LIMIT` → `LIMIT`：DAY → ROD；GTC → ROD（warning）。
- LIMIT 價格仍會 snap 到 tick grid（既有）。

### 3.5 邊界 / 錯誤處理

- **Tags 保留**：emulated stop 釋放出的單應保留 `order.tags`，使「停損觸發後送
  融資/零股單」可行 → 列為測試項（§3.6）。
- **Trigger 型別**：建議文件以 `LAST_PRICE`（成交價）為台股/台期預設；`BID_ASK`
  亦支援。adapter 端不需驗證（emulation 為策略側決定）。
- **行程當機空窗**：觸發在 NT 行程內，當機期間不觸發。被 hold 的 emulated 單存於
  cache；**建議用 Redis cache** 讓 emulated 單跨重啟復原。此限制寫進文件。

### 3.6 測試（pragmatic-testing：重行為，不重儀式）

單元測試（`tests/integration_tests/adapters/sinopac/test_execution.py`）：
1. 裸 `STOP_MARKET`/`STOP_LIMIT`/`MARKET_IF_TOUCHED`/`LIMIT_IF_TOUCHED`/
   `TRAILING_STOP_*` → 收到 `OrderRejected`，reason 含引導訊息（**回歸價值最高**：
   目前為靜默丟棄）。
2. 真正未知型別 → `OrderRejected` 帶泛用訊息。
3. 模擬觸發後的 `MARKET` 帶 `SinopacOrderTags(order_cond="MarginTrading")` →
   送出時 `order_cond` 正確保留。

範例（`examples/live/sinopac/sinopac_exec_tester.py`，sim/dry-run）：
4. 一張 emulated `STOP_MARKET`（`LAST_PRICE`）：觀察 emulate → trigger → submit。
5. 一組 bracket（entry + SL + TP，OCO）：觀察子單在觸發後才送出。

---

## 4. Part B — 整合文件完整能力稽核（#3）

對 `docs/integrations/sinopac.md` 逐段對照程式碼修正。

### 4.1 已確認的 drift 清單

| 文件位置 | 現況 | 事實（程式碼） |
|---|---|---|
| L181-184 Order types 矩陣 | 僅列 `MARKET`/`LIMIT` | 另有 MKP（期/選）；無原生 stop |
| L193-197 | 「`_submit_order` does not send an `order_lot` field」 | **錯**：`execution.py:1004-1009` 一律送 `order_lot`（預設 Common） |
| L208-213 | odd-lot/intraday-odd「**not implemented**」 | **錯**：`IntradayOdd` 經 `SinopacOrderTags(order_lot=...)` 已可用（`execution.py:236, 346, 1005`） |
| 全文 | 未提 `order_cond`/margin/short、`daytrade_short`、`octype`、MKP | 四者皆已實作（`execution.py:1004-1009`） |

canonical 能力矩陣在 `nautilus_trader/adapters/sinopac/__init__.py:15-111`。

### 4.2 修正計畫（逐段）

1. **Order types 章節**：補全 — `MARKET`/`LIMIT`（原生）、`MARKET_TO_LIMIT`/MKP
   （期/選限定，股票本地拒絕）；**新增「Stop / conditional orders（emulated only）」
   小節**，含 `emulation_trigger` 用法、支援的 trigger（`LAST_PRICE`/`BID_ASK`）、
   行程當機空窗限制與 Redis cache 建議。
2. **Odd-lot 章節**：刪除錯誤的「not implemented」警告，改為：`IntradayOdd` 已支援
   （經 tags）；`Odd`（盤後零股）與 `Fixing`（定盤）仍 out of scope（backlog B3）。
3. **新增「Taiwan order tags」能力矩陣**：`order_cond`（Cash/MarginTrading/
   ShortSelling）、`daytrade_short`、`octype`（Auto/New/Cover/DayTrade，期/選）。
4. **單一真相來源**：能力矩陣以 `__init__.py` 為 canonical，整合文件交叉連結並避免
   重複敘述，降低未來再 drift 風險。

---

## 5. 範圍邊界（YAGNI，明確不做）

- gateway 常駐觸發引擎（方案 B）—— 未來 resilience backlog。
- 盤後 `Odd` 零股、`Fixing` 定盤單 —— backlog B3。
- TAIFEX combo/spread 單、reserve 單 —— backlog B3。
- 不修改 gateway（`shioaji-server`）。

---

## 6. 驗收條件（Acceptance Criteria）

- [ ] 裸停損/條件單（含 bracket 子單）→ 發出 `OrderRejected` 帶引導訊息，不再靜默丟棄。
- [ ] emulated `STOP_MARKET`/`STOP_LIMIT` 觸發後正常送出 `MARKET`/`LIMIT`，TIF 正確 coerce。
- [ ] emulated stop 釋放出的單保留 tags（margin 範例通過）。
- [ ] `sinopac_exec_tester.py` 新增 emulated STOP + bracket 的 sim 場景並可跑通。
- [ ] 新增/修正的單元測試全綠（`uv run pytest tests/integration_tests/adapters/sinopac/`）。
- [ ] `docs/integrations/sinopac.md` 與程式碼一致：order types、odd-lot、tags 矩陣、stop 章節皆更新。

---

## 7. 參考

- `nautilus_trader/adapters/sinopac/execution.py`（`_submit_order` 944-946、
  `_resolve_order_type` 185-228、tags 1004-1009、`_submit_order_list` 1186-1195）
- `nautilus_trader/adapters/sinopac/__init__.py:15-111`（canonical 能力矩陣）
- `nautilus_trader/execution/emulator.pyx`（78、356-446）
- `docs/integrations/sinopac.md:181-213`（待修正 drift）
- Shioaji `references/ADVANCED.md`（Stop Orders 為客戶端模擬）

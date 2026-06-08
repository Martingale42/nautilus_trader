# Batch 3 程式碼審查 — Sinopac 執行修復 (P1–P3)

- 日期：2026-06-08
- 分支：`sinopac-adapter-clean`
- 審查 commits：`ed362e2d69` (P2)、`4f6bb70253` (P3)、`d47036a79c` (P1)
- 審查者：Code Reviewer

## 結論：**CHANGES REQUESTED**

P2、P3 方向正確、可接受（P3 有一個 Important 的對帳缺口需註記）。
**P1 有一個 Critical 的金融正確性缺陷：所選的唯一鍵 `seqno` 在成交事件上「不是」逐筆唯一，
本次修復不但沒有修好重複 TradeId 的 bug，反而很可能把原本正確的行為改壞。**

---

## Critical

### C1 — P1 用錯欄位：`seqno` 在成交回報中是「逐單」而非「逐筆」 `[execution.py:336]` `[order_parse.rs:76]`

修復把成交 TradeId 從 `f"{trade_id}-{ordno}"` 改成 `f"{trade_id}-{seqno}"`，
前提是「`ordno` 在同一張單的所有 partial fill 之間相同、`seqno` 逐筆唯一」。
**這個前提與 Shioaji 官方文件相反。**

官方文件（`sinotrade.github.io/tutor/order_deal_event/stocks/` 與 `.../futures/`）明確說明，
對「成交事件（StockDeal / FuturesDeal）」而言：

- `trade_id` == 委託的 `id` → **同一張單的所有成交相同**（逐單）。
- `seqno` == 委託的 `seqno` →「the seqno in the order is the same as seqno in the deals」→ **同一張單的所有成交相同**（逐單）。
- `ordno`（成交事件上的）== 前 5 碼為委託 ordno，**後 3 碼為成交序號（001、002、003…）** →「the last 3 characters represent the deal sequence number」→ **逐筆唯一**。
- `exchange_seq` → 交易所逐筆序號 → 逐筆唯一。

也就是說，成交事件的 `ordno`（原始碼用的欄位）本來就逐筆唯一，而 `seqno`（修復改用的欄位）才是逐單相同的那一個。
方向剛好接反。

**真實資料實證**（以官方語義建構兩筆 partial fill，`seqno` 相同、`ordno` 後綴遞增）：

```
=== NEW code (seqno-based) on REAL data ===
  fill1: abc123-123456
  fill2: abc123-123456
  distinct? False  <-- 碰撞 → 帳本仍然損毀
=== OLD code (ordno-based) on REAL data ===
  fill1: abc123-tA0deX001
  fill2: abc123-tA0deX002
  distinct? True
```

新碼在真實資料上產生「重複 TradeId」，NT 成交去重會丟棄第二筆以後的成交 → 持倉/已成交量錯誤。
這正是 P1 想消滅的 bug，結果換了一個欄位後 bug 依舊存在；而且若原本 `ordno` 走的是成交層級的 8 碼
ordno（gateway `client.py:378` `on_order` 把 `msg` 原封不動轉發，Rust `messages.rs:311` 直接反序列化
`StockDealEventData.ordno`，無轉換），原始碼其實是正確的，本次修復屬於**回歸**。

**測試為何仍綠**：`test_p1_partial_fills_same_ordno_distinct_seqno_yield_distinct_trade_ids`
（`test_execution.py:103`）刻意建構 `ordno` 相同、`seqno`（`000001`/`000002`）不同的事件——
這是**與真實 Shioaji 資料相反的杜撰輸入**。測試本身對「修復機制」非套套邏輯（pre-fix 兩筆都會
得到 `T0001-A1234`，斷言會失敗），但因為輸入語義錯誤，它證明的是一個現實中不存在的情境，
無法保護真正的帳本正確性。Rust 單元測試（`order_parse.rs:166`）只驗 `seqno != ordno` 且 dict 含 `seqno`，
不觸及逐筆唯一性，同樣無法把關。

**建議修法（擇一，需先以模擬/真倉抓一筆 multi-fill 回報實證欄位值再定案）：**

1. 成交層級 `ordno` 若確為 8 碼逐筆唯一 → **沿用原始 `ordno`**（即 revert P1 的 Python 改動），
   或顯式記錄「deal-level ordno 已含成交序號」的註解，避免日後再被誤改。
2. 若要更穩健，改用 `exchange_seq`（交易所逐筆序號，官方說逐筆唯一），fallback 到 `ordno`：
   `seq = event.get("exchange_seq") or ordno`。`seqno` **不可**作為唯一鍵。
3. 對應地把 Rust `set_deal_fields` 改為透傳 `exchange_seq`（型別為 `Option<String>`，
   見 `messages.rs:313`，反序列化已是安全路徑，但 Python 端組鍵需處理 `None`）。

**並請同步修正測試**：用符合官方語義的輸入——`seqno` 與 `trade_id` 兩筆相同、唯一鍵欄位
（`exchange_seq` 或 deal-level `ordno`）兩筆不同——否則綠燈無意義。

---

## Important

### I2 — P3 對帳「adopt」路徑未完整，超時單可能殘留 SUBMITTED 並產生重影 `[execution.py:447]` `[execution.py:620-633]`

P3 把 transport 失敗（`asyncio.TimeoutError`, `OSError`）從「一律 reject」改為「保留 SUBMITTED 待對帳」，
方向正確、消除了「回報 REJECTED 但 venue 持有活單」的隱性曝險，這是真實的改善。

但 commit body 宣稱「`generate_order_status_reports` 以 venue trade_id 為鍵重建 OrderStatusReport，
NT 對帳可據此 adopt 該訂單，不會永久卡在 SUBMITTED」——此路徑只**部分**成立：

- 對帳確實存在（`execution.py:580` `generate_order_status_reports` → `list_trades`），
  且在 `_trade_id_to_client_order_id` 缺失時回退合成 `ClientOrderId(f"SINOPAC-{trade_id}")`（`:624`），
  `venue_order_id=VenueOrderId(trade_id)`（`:633`）。✓
- **缺口**：超時時本地 SUBMITTED 單**沒有 venue_order_id**（`trade_id` 在 `place_order` 回應裡，已逾時收不到），
  而對帳報告帶的是合成 `client_order_id` + venue `trade_id`。NT 對帳主要以 `venue_order_id` 比對既有單；
  本地單無 venue_order_id 可比對，合成 client_order_id 又與本地單不同 →
  **較可能被 NT 當成「外部單」新建**，而非 adopt 既有的 SUBMITTED 單。
  結果：原本地單可能仍卡 SUBMITTED，外加一張重影外部單。

這不是阻擋級（P3 仍優於原本的假 REJECT），但「乾淨 adopt」的宣稱證據不足。建議：
（a）將 commit/註解的措辭降級為「對帳會以 trade_id 重建 OrderStatusReport，後續成交可被關聯；
本地原單的收斂依賴 NT 既有外部單合併行為」；或（b）參考 Hyperliquid adapter 在
`submit_order` 階段先以 client_order_id 暫存 venue 對應、或在對帳時把合成單對應回 client_order_id，
才能真正 adopt。請至少加一條對帳測試覆蓋「逾時單 → list_trades 回 working → 對帳後本地單狀態收斂」。

---

## Minor

### M3 — `seq = event.get("seqno") or ordno` 的 falsy fallback `[execution.py:336]`

任務特別點名此 edge case：若 `seqno` 可能合法為 `0`／空字串，`or` 會誤回退到 `ordno`。
經查 Shioaji `seqno` 型別為 `str`（`messages.rs:309` Rust `String`、官方 TypedDict `seqno: str`），
實務值如 `"123456"`，不會是整數 `0`；空字串本身也無意義，故 `or` 在現行型別下**無害**。
**此邊界情況已評估：不構成獨立缺陷。** 但因 C1 要改用別的欄位，屆時若改用 `exchange_seq`
（型別 `Option<String>`），請改用顯式 `event.get("exchange_seq") or ordno` 並確認 `None` 處理，
不要用 `.get(key, default)` 配 `or` 混用。

### M4 — P2 guard 含 `OrderStatus.FILLED` 為實質不可達 `[execution.py:266]`

`FILLED` 為末態，且全部成交時 `_trade_id_to_client_order_id` 已於 `:386` pop，
故 `:241` 的 mapping 查詢會回 None、提早 return，不會走到 guard。納入 `FILLED` 屬防禦性、無害，
不需修改，僅記錄。

---

## 各項檢查表

### P1（金融正確性）

- Rust `set_deal_fields` 確實把 `seqno` 寫入 Python dict（stock/futures 兩路徑）`[order_parse.rs:76,109,135]`。✓ 機制成立。
- 型別安全：`StockDealEventData.seqno`/`FuturesDealEventData.seqno` 為必填 `String`（`messages.rs:309,356`），
  `dict.set_item("seqno", seqno)?` 以 `?` 傳播錯誤，**外部資料無 `unwrap()`/`expect()` 可 panic 即時行情**。✓
- **逐筆唯一性：FAIL**——`seqno` 為逐單相同（見 C1），非逐筆唯一。✗（Critical）
- 測試證明性：pre-fix 會碰撞、post-fix 不碰撞，對機制非套套邏輯；**但輸入語義與真實資料相反**，
  無法保護真正情境（見 C1）。✗

### P2（非法狀態轉移）

- guard 在 reject 前先查快取 `order.status`（`:247` 取單、`:263` 判斷）。✓
- `order is None` 已於 `:248` 提早 return，guard 無 NPE。✓
- `ACCEPTED/PARTIALLY_FILLED/FILLED → REJECTED` 不再發生；後到事件記 warning + drop、保留 mapping（不 pop）。✓
- 仍 PENDING/SUBMITTED 的單仍可正常 reject，無誤吞（`test_p2_new_failure_before_accept_still_rejects`）。✓

### P3（超時保留訂單）

- transport 失敗（`asyncio.TimeoutError`, `OSError`）不再呼叫 `generate_order_rejected`（`:438` except 分流）。✓
- 真正 business 例外仍 reject（`test_p3_business_rejection_still_rejects`）。✓
- 對帳路徑存在（`generate_order_status_reports` → `list_trades`，`:580`）。✓（但 adopt 不完整，見 I2）
- 不會永久卡 SUBMITTED：**證據不足**，見 I2。△

### 通用

- Rust diff 無 `unwrap()`/`expect()` 作用在外部資料上（測試碼內的 `.expect()` 屬測試斷言，可接受）。✓
- 無 YAGNI 違規；測試有意義但 P1 測試輸入語義錯誤（見 C1）。△
- 無 `Cargo.lock`/`uv.lock` 或任何 shioaji-server 檔案被 stage：三個 commit 僅動
  `execution.py`、`order_parse.rs`、`test_execution.py`。✓ 無跨倉污染。

---

## 驗證輸出

### `cargo test -p nautilus-sinopac --features python`（tail）

```
test websocket::order_parse::tests::test_deal_pydict_contains_unique_seqno ... ok
test result: ok. 72 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
...
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   (tests/http.rs)
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   (tests/websocket.rs)
```

### `uv run --active --no-sync pytest tests/integration_tests/adapters/sinopac/ -v`（tail）

```
test_p1_partial_fills_same_ordno_distinct_seqno_yield_distinct_trade_ids PASSED
test_p1_falls_back_to_ordno_when_seqno_absent PASSED
test_p2_late_new_failure_on_accepted_order_is_ignored PASSED
test_p2_new_failure_before_accept_still_rejects PASSED
test_p3_timeout_does_not_reject_order PASSED
test_p3_business_rejection_still_rejects PASSED
... (config/factories) ...
============================== 16 passed in 0.13s ==============================
```

> 全綠，但 P1 綠燈基於與真實 Shioaji 語義相反的杜撰輸入，不代表帳本正確性已修復（見 C1）。

---

## 待辦（合併前需處理）

1. **[Critical] C1**：P1 改用真正逐筆唯一的鍵（建議 `exchange_seq`，或確認後沿用 deal-level `ordno`）；
   `seqno` 不可作唯一鍵。先以模擬/真倉抓一筆 multi-fill 成交回報實證欄位值。
2. **[Critical] C1**：同步修正 P1 測試輸入，使其符合官方語義（`seqno`/`trade_id` 跨筆相同、唯一鍵欄位跨筆不同）。
3. **[Important] I2**：修正 P3 commit/註解對「adopt」的過度宣稱，或補實對帳收斂路徑 + 一條對帳測試。

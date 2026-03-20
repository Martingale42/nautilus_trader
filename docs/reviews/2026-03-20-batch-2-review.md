# Batch 2 Review (Tasks 5-8)

**Date:** 2026-03-20
**Reviewer:** Claude (Code Reviewer)
**Commits:** 26ad940f0, 3c037fee2
**Plan:** `docs/plans/2026-03-20-sinopac-structural-refactor.md`

## Verdict: APPROVED WITH NOTES

The migration from custom `tokio-tungstenite` WebSocket handling to `nautilus_network::WebSocketClient` is correctly implemented. Reconnection, re-subscription, disconnect, and the Python bindings all work as intended. Tests pass, clippy is clean, formatting is clean. Two important findings noted below -- neither blocks approval, but both should be addressed in follow-up work.

---

## Verification Results

| Check | Result |
|-------|--------|
| `cargo clippy -p nautilus-sinopac --all-targets` | Clean (no warnings) |
| `cargo +nightly fmt -p nautilus-sinopac -- --check` | Clean |
| `cargo test -p nautilus-sinopac` | 71/71 passed (63 unit + 5 HTTP + 3 WS) |
| No `tracing::` usage | Clean |
| No bare `#[test]` (all `#[rstest]`) | Clean |
| `disconnect()` testable | Yes -- `test_connect_disconnect` now calls `disconnect().await` successfully |

---

## Task-by-Task Review

### Task 5: Rewrite SinopacWebSocketClient to wrap nautilus_network (26ad940f0)

**Files:** `websocket/client.rs`, `websocket/handler.rs`, `websocket/mod.rs`

- `SinopacWebSocketClient` now wraps `nautilus_network::WebSocketClient` behind `Arc<tokio::sync::Mutex<Option<WebSocketClient>>>`
- `connect()` correctly uses `channel_message_handler()` to get `(MessageHandler, UnboundedReceiver<Message>)`, passes handler to `WebSocketClient::connect()`, spawns feed handler
- `subscribe()`/`unsubscribe()` correctly serialize `WsSubscribeMsg` and send via `ws_client.send_text()` directly (no command channel needed)
- Subscription tracking via `HashSet<(String, SinopacQuoteType)>` enables post-reconnection re-subscribe
- `disconnect()` correctly aborts feed handler task then disconnects network client
- `is_connected()` correctly checks `ConnectionMode::Active`
- `WsCommand` enum fully removed from `mod.rs` since subscribe/unsubscribe are now direct sends
- `handler.rs` rewritten as `feed_handler()` -- detects `nautilus_network::RECONNECTED` sentinel and calls `resubscribe_all()`
- Manual `Clone` impl necessary because `WebSocketClient` does not derive `Clone`
- Copyright headers present, `///` docs on all pub items, `log::` (not `tracing::`)

### Task 6: Update Python WS bindings for new client (26ad940f0)

**Files:** `python/websocket.rs`

- `py_subscribe`, `py_unsubscribe`, `py_is_connected` correctly changed from synchronous to async (`future_into_py`)
- `py_connect` still takes `msg_rx` from client and spawns callback loop via `get_runtime().spawn()`
- `py_disconnect` delegates to `client.disconnect().await`
- `py_wait_until_active` correctly uses `client.is_connected().await` in polling loop
- Doc comments use indicative mood ("Subscribes...", "Returns...", "Connects...", "Disconnects...")

### Task 7: Remove tokio-tungstenite direct dependency (3c037fee2)

**Files:** `Cargo.toml`

- `futures-util` correctly moved from `[dependencies]` to `[dev-dependencies]` (only used in test mock server)
- `tokio-tungstenite` correctly retained as direct dependency because `handler.rs` uses `tungstenite::Message` type from the `channel_message_handler()` receiver. This type is not re-exported by `nautilus_network`, so the direct dependency is justified.

### Task 8: Final standards check and integration test update (26ad940f0)

**Files:** `tests/websocket.rs`, `data.py`

- `test_connect_disconnect` now calls `client.disconnect().await` -- the workaround comment about cross-runtime deadlock has been removed
- `is_connected()` calls updated from sync to async throughout tests: `client.is_connected().await`
- `subscribe()` calls updated to async: `.subscribe(...).await`
- `wait_until_async` closures correctly clone client for use in async block
- `data.py` subscribe/unsubscribe calls correctly updated with `await`
- `data.py` `_disconnect` correctly uses `await self._ws_client.is_connected()`

---

## Findings

### Important

1. **[client.rs:128-134] Race condition: feed handler spawned before `ws_client` is stored**

   At line 130, the feed handler is spawned with `ws_client_ref` (a clone of `Arc<tokio::sync::Mutex<Option<WebSocketClient>>>`). However, the actual `WebSocketClient` is not stored in the mutex until line 134 (`*self.ws_client.lock().await = Some(client)`). If `nautilus_network` delivers a `__RECONNECTED__` sentinel between spawn and storage, `resubscribe_all()` will find `None` in the mutex and silently skip re-subscription.

   In practice this race is extremely unlikely -- reconnection happens on connection loss, not immediately after initial connect. The initial connect hasn't even been fully set up yet at the point of spawn. However, the fix is trivial: store the client in the mutex **before** spawning the feed handler. This would also make the code easier to reason about.

2. **[handler.rs:104] Lock acquired per-iteration in `resubscribe_all` loop**

   The `ws_client` tokio mutex is acquired and released for each subscription in the loop (line 104). This is functionally correct (tokio mutexes are designed for holding across `.await`), but acquiring the lock once outside the loop and sending all subscribe messages would be more efficient and would prevent interleaving with user-initiated subscribe/unsubscribe calls during re-subscription. Consider:
   ```rust
   let guard = ws_client.lock().await;
   if let Some(client) = guard.as_ref() {
       for (code, quote_type) in subs_snapshot { ... }
   }
   ```

### Minor

3. **[client.rs:130] `tokio::spawn` used in non-test production code**

   The project convention (observed in Bybit adapter) is to use `get_runtime().spawn()` rather than bare `tokio::spawn`. Since `connect()` is always called from within the pyo3-async-runtimes tokio runtime, `tokio::spawn` works correctly here. However, for consistency with the rest of the codebase, consider using `get_runtime().spawn()`.

4. **[Tasks 5+6+8 merged into single commit]**

   The plan specified separate commits for Tasks 5, 6, and 8. All three were merged into commit 26ad940f0. This is acceptable since the changes are tightly coupled (the Python bindings cannot compile without the Rust changes), but it makes individual task verification slightly harder in the git history.

---

## Security

No security concerns. The WebSocket URL is configured, not constructed from user input. No injection or traversal vectors. Auth is handled at the gateway level.

## API Conformance

Wire format is unchanged. `WsSubscribeMsg` serialization produces the same JSON as before (verified by passing integration tests with mock server).

---

## Summary

The nautilus_network migration is well-executed. The core changes follow the plan accurately: custom `tokio-tungstenite` handling is replaced with `nautilus_network::WebSocketClient`, reconnection support is added via sentinel detection, subscription tracking enables automatic re-subscribe, and `disconnect()` is now fully functional (no cross-runtime issues). All verification checks pass. The two important findings are low-risk improvements that can be addressed in a follow-up commit.

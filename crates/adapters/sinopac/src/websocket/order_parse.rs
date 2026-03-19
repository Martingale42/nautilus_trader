// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  You may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
// -------------------------------------------------------------------------------------------------

//! Parsers for Sinopac WebSocket order events.

use pyo3::{prelude::*, types::PyDict};

use super::messages::{OperationInfo, OrderEvent, OrderStatusInfo};

/// Sets order status fields common to both stock and futures order events.
fn set_order_fields(
    dict: &Bound<'_, PyDict>,
    event_type: &str,
    op: &OperationInfo,
    order_id: &str,
    ordno: &str,
    action: &str,
    price: f64,
    quantity: i64,
    order_type: &str,
    price_type: &str,
    code: &str,
    status: &OrderStatusInfo,
) -> PyResult<()> {
    dict.set_item("event_type", event_type)?;
    dict.set_item("op_type", &op.op_type)?;
    dict.set_item("op_code", &op.op_code)?;
    dict.set_item("op_msg", &op.op_msg)?;
    dict.set_item("order_id", order_id)?;
    dict.set_item("ordno", ordno)?;
    dict.set_item("action", action)?;
    dict.set_item("price", price)?;
    dict.set_item("quantity", quantity)?;
    dict.set_item("order_type", order_type)?;
    dict.set_item("price_type", price_type)?;
    dict.set_item("code", code)?;
    dict.set_item("exchange_ts", status.exchange_ts)?;
    dict.set_item("cancel_quantity", status.cancel_quantity)?;
    dict.set_item("order_quantity", status.order_quantity)?;
    dict.set_item("modified_price", status.modified_price)?;
    Ok(())
}

/// Sets deal (fill) fields common to both stock and futures deal events.
fn set_deal_fields(
    dict: &Bound<'_, PyDict>,
    event_type: &str,
    trade_id: &str,
    ordno: &str,
    action: &str,
    code: &str,
    price: f64,
    quantity: i64,
    ts: f64,
) -> PyResult<()> {
    dict.set_item("event_type", event_type)?;
    dict.set_item("trade_id", trade_id)?;
    dict.set_item("ordno", ordno)?;
    dict.set_item("action", action)?;
    dict.set_item("code", code)?;
    dict.set_item("price", price)?;
    dict.set_item("quantity", quantity)?;
    dict.set_item("ts", ts)?;
    Ok(())
}

/// Converts an [`OrderEvent`] to a Python dict for the execution client to process.
pub fn order_event_to_pydict(py: Python<'_>, event: &OrderEvent) -> PyResult<Py<PyDict>> {
    let dict = PyDict::new(py);

    match event {
        OrderEvent::StockOrder(data) => set_order_fields(
            &dict,
            "stock_order",
            &data.operation,
            &data.order.id,
            &data.order.ordno,
            &data.order.action,
            data.order.price,
            data.order.quantity,
            &data.order.order_type,
            &data.order.price_type,
            &data.contract.code,
            &data.status,
        )?,
        OrderEvent::StockDeal(data) => set_deal_fields(
            &dict,
            "stock_deal",
            &data.trade_id,
            &data.ordno,
            &data.action,
            &data.code,
            data.price,
            data.quantity,
            data.ts,
        )?,
        OrderEvent::FuturesOrder(data) => set_order_fields(
            &dict,
            "futures_order",
            &data.operation,
            &data.order.id,
            &data.order.ordno,
            &data.order.action,
            data.order.price,
            data.order.quantity,
            &data.order.order_type,
            &data.order.price_type,
            &data.contract.code,
            &data.status,
        )?,
        OrderEvent::FuturesDeal(data) => set_deal_fields(
            &dict,
            "futures_deal",
            &data.trade_id,
            &data.ordno,
            &data.action,
            &data.code,
            data.price,
            data.quantity,
            data.ts,
        )?,
    }

    Ok(dict.into())
}

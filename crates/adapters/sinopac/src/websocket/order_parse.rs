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

use super::messages::OrderEvent;

/// Converts an [`OrderEvent`] to a Python dict for the execution client to process.
pub fn order_event_to_pydict(py: Python<'_>, event: &OrderEvent) -> PyResult<Py<PyDict>> {
    let dict = PyDict::new(py);

    match event {
        OrderEvent::StockOrder(data) => {
            dict.set_item("event_type", "stock_order")?;
            dict.set_item("op_type", &data.operation.op_type)?;
            dict.set_item("op_code", &data.operation.op_code)?;
            dict.set_item("op_msg", &data.operation.op_msg)?;
            dict.set_item("order_id", &data.order.id)?;
            dict.set_item("ordno", &data.order.ordno)?;
            dict.set_item("action", &data.order.action)?;
            dict.set_item("price", data.order.price)?;
            dict.set_item("quantity", data.order.quantity)?;
            dict.set_item("order_type", &data.order.order_type)?;
            dict.set_item("price_type", &data.order.price_type)?;
            dict.set_item("code", &data.contract.code)?;
            dict.set_item("exchange_ts", data.status.exchange_ts)?;
            dict.set_item("cancel_quantity", data.status.cancel_quantity)?;
            dict.set_item("order_quantity", data.status.order_quantity)?;
            dict.set_item("modified_price", data.status.modified_price)?;
        }
        OrderEvent::StockDeal(data) => {
            dict.set_item("event_type", "stock_deal")?;
            dict.set_item("trade_id", &data.trade_id)?;
            dict.set_item("ordno", &data.ordno)?;
            dict.set_item("action", &data.action)?;
            dict.set_item("code", &data.code)?;
            dict.set_item("price", data.price)?;
            dict.set_item("quantity", data.quantity)?;
            dict.set_item("ts", data.ts)?;
        }
        OrderEvent::FuturesOrder(data) => {
            dict.set_item("event_type", "futures_order")?;
            dict.set_item("op_type", &data.operation.op_type)?;
            dict.set_item("op_code", &data.operation.op_code)?;
            dict.set_item("op_msg", &data.operation.op_msg)?;
            dict.set_item("order_id", &data.order.id)?;
            dict.set_item("ordno", &data.order.ordno)?;
            dict.set_item("action", &data.order.action)?;
            dict.set_item("price", data.order.price)?;
            dict.set_item("quantity", data.order.quantity)?;
            dict.set_item("order_type", &data.order.order_type)?;
            dict.set_item("price_type", &data.order.price_type)?;
            dict.set_item("code", &data.contract.code)?;
            dict.set_item("exchange_ts", data.status.exchange_ts)?;
            dict.set_item("cancel_quantity", data.status.cancel_quantity)?;
            dict.set_item("order_quantity", data.status.order_quantity)?;
            dict.set_item("modified_price", data.status.modified_price)?;
        }
        OrderEvent::FuturesDeal(data) => {
            dict.set_item("event_type", "futures_deal")?;
            dict.set_item("trade_id", &data.trade_id)?;
            dict.set_item("ordno", &data.ordno)?;
            dict.set_item("action", &data.action)?;
            dict.set_item("code", &data.code)?;
            dict.set_item("price", data.price)?;
            dict.set_item("quantity", data.quantity)?;
            dict.set_item("ts", data.ts)?;
        }
    }

    Ok(dict.into())
}

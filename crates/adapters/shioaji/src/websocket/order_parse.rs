use pyo3::prelude::*;
use pyo3::types::PyDict;

use super::messages::OrderEvent;

/// Convert an [`OrderEvent`] to a Python dict for the execution client to process.
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

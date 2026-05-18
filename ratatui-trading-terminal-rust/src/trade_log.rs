use chrono::Utc;
use std::fs::OpenOptions;
use std::path::Path;

use crate::api::{Order, OrderRequest};

const CSV_PATH: &str = "trades.csv";

pub fn log_trade(req: &OrderRequest, order: &Order) {
    let is_new = !Path::new(CSV_PATH).exists();
    let file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(CSV_PATH);
    let Ok(file) = file else {
        return;
    };
    let mut wtr = csv::WriterBuilder::new().has_headers(false).from_writer(file);

    if is_new {
        let _ = wtr.write_record([
            "timestamp",
            "symbol",
            "side",
            "type",
            "qty",
            "limit_price",
            "order_id",
            "status",
        ]);
    }
    let _ = wtr.write_record([
        Utc::now().to_rfc3339(),
        order.symbol.clone(),
        order.side.clone(),
        order.order_type.clone(),
        order.qty.clone(),
        req.limit_price.clone(),
        order.id.clone(),
        order.status.clone(),
    ]);
    let _ = wtr.flush();
}

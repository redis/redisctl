//! Cloud payment method command implementations

use anyhow::Context;
use redis_cloud::AccountHandler;
use serde_json::Value;
use tabled::{Table, settings::Style};

use crate::cli::{CloudPaymentMethodCommands, OutputFormat};
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;

use super::utils::*;

/// Handle cloud payment method commands
pub async fn handle_payment_method_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &CloudPaymentMethodCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    match command {
        CloudPaymentMethodCommands::List => {
            list_payment_methods(conn_mgr, profile_name, output_format, query).await
        }
    }
}

/// List payment methods configured for the account
async fn list_payment_methods(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;
    let handler = AccountHandler::new(client);

    let response = handler
        .get_account_payment_methods()
        .await
        .context("Failed to fetch payment methods")?;

    let json_value = serde_json::to_value(response)?;
    let data = handle_output(json_value, output_format, query)?;

    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            print_payment_methods_table(&data)?;
        }
        _ => print_formatted_output(data, output_format)?,
    }

    Ok(())
}

/// Print payment methods in table format
fn print_payment_methods_table(data: &Value) -> CliResult<()> {
    let methods = data.get("paymentMethods").and_then(|p| p.as_array());

    if let Some(methods) = methods {
        if methods.is_empty() {
            println!("No payment methods configured");
            return Ok(());
        }

        let mut rows = Vec::new();
        for method in methods {
            let id = method.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            let type_ = method
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            let last4 = method
                .get("last4Digits")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let exp = method
                .get("expirationDate")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            rows.push(PaymentMethodRow {
                id: id.to_string(),
                payment_type: type_.to_string(),
                last_4: last4.to_string(),
                expiration: exp.to_string(),
            });
        }

        let mut table = Table::new(&rows);
        table.with(Style::blank());
        output_with_pager(&table.to_string());
    } else {
        println!("No payment methods data available");
    }
    Ok(())
}

// Table row structure for formatting
#[derive(tabled::Tabled)]
struct PaymentMethodRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Type")]
    payment_type: String,
    #[tabled(rename = "Last 4")]
    last_4: String,
    #[tabled(rename = "Expiration")]
    expiration: String,
}

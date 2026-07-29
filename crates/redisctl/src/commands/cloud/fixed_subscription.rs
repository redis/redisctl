//! Fixed subscription command implementations

use crate::cli::{CloudFixedSubscriptionCommands, OutputFormat};
use crate::commands::cloud::async_utils::handle_async_response;
use crate::commands::cloud::utils::{confirm_action, handle_output, print_formatted_output};
use crate::connection::ConnectionManager;
use crate::error::{RedisCtlError, Result as CliResult};
use anyhow::Context;
use redis_cloud::fixed::subscriptions::{
    FixedSubscriptionCreateRequest, FixedSubscriptionHandler, FixedSubscriptionUpdateRequest,
};

/// Read JSON data from string or file (prefixed with @)
fn read_json_data(data: &str) -> CliResult<serde_json::Value> {
    let json_str = if let Some(file_path) = data.strip_prefix('@') {
        std::fs::read_to_string(file_path).map_err(|e| RedisCtlError::InvalidInput {
            message: format!("Failed to read file {}: {}", file_path, e),
        })?
    } else {
        data.to_string()
    };
    serde_json::from_str(&json_str).map_err(|e| RedisCtlError::InvalidInput {
        message: format!("Invalid JSON: {}", e),
    })
}

/// Handle fixed subscription commands
pub async fn handle_fixed_subscription_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &CloudFixedSubscriptionCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr
        .create_cloud_client(profile_name)
        .await
        .context("Failed to create Cloud client")?;

    let handler = FixedSubscriptionHandler::new(client);

    match command {
        CloudFixedSubscriptionCommands::ListPlans { provider } => {
            let plans = if let Some(provider_filter) = provider {
                // If provider specified, fetch all plans and filter
                let all_plans = handler
                    .list_plans(None, None)
                    .await
                    .context("Failed to list fixed subscription plans")?;

                // Convert to JSON for filtering
                let mut json_plans = serde_json::to_value(all_plans)?;

                // Filter by provider if the structure supports it
                if let Some(plans_array) = json_plans.as_array_mut() {
                    plans_array.retain(|plan| {
                        plan.get("cloudProvider")
                            .and_then(|p| p.as_str())
                            .map(|p| p.eq_ignore_ascii_case(provider_filter))
                            .unwrap_or(false)
                    });
                }

                json_plans
            } else {
                // No filter, get all plans
                let plans = handler
                    .list_plans(None, None)
                    .await
                    .context("Failed to list fixed subscription plans")?;
                serde_json::to_value(plans)?
            };

            let data = handle_output(plans, output_format, query)?;
            print_formatted_output(data, output_format)?;
            Ok(())
        }

        CloudFixedSubscriptionCommands::GetPlans { subscription } => {
            let plans = handler
                .get_plans_by_subscription_id(*subscription)
                .await
                .context("Failed to get subscription plans")?;

            let json_response =
                serde_json::to_value(plans).context("Failed to serialize response")?;
            let data = handle_output(json_response, output_format, query)?;
            print_formatted_output(data, output_format)?;
            Ok(())
        }

        CloudFixedSubscriptionCommands::GetPlan { id } => {
            let plan = handler
                .get_plan_by_id(*id)
                .await
                .context("Failed to get plan details")?;

            let json_response =
                serde_json::to_value(plan).context("Failed to serialize response")?;
            let data = handle_output(json_response, output_format, query)?;
            print_formatted_output(data, output_format)?;
            Ok(())
        }

        CloudFixedSubscriptionCommands::List => {
            let subscriptions = handler
                .list()
                .await
                .context("Failed to list fixed subscriptions")?;

            let json_response =
                serde_json::to_value(subscriptions).context("Failed to serialize response")?;
            let data = handle_output(json_response, output_format, query)?;
            print_formatted_output(data, output_format)?;
            Ok(())
        }

        CloudFixedSubscriptionCommands::Get { id } => {
            let subscription = handler
                .get_by_id(*id)
                .await
                .context("Failed to get fixed subscription")?;

            let json_response =
                serde_json::to_value(subscription).context("Failed to serialize response")?;
            let data = handle_output(json_response, output_format, query)?;
            print_formatted_output(data, output_format)?;
            Ok(())
        }

        CloudFixedSubscriptionCommands::Create {
            name,
            plan_id,
            payment_method,
            payment_method_id,
            data,
            async_ops,
        } => {
            // Start with data from --data if provided, otherwise empty object
            let mut request_value = if let Some(data_str) = data {
                read_json_data(data_str)?
            } else {
                serde_json::json!({})
            };

            let request_obj =
                request_value
                    .as_object_mut()
                    .ok_or_else(|| RedisCtlError::InvalidInput {
                        message: "JSON data must be an object".to_string(),
                    })?;

            // Apply first-class params (override JSON values)
            if let Some(n) = name {
                request_obj.insert("name".to_string(), serde_json::json!(n));
            }
            if let Some(p) = plan_id {
                request_obj.insert("planId".to_string(), serde_json::json!(p));
            }
            if let Some(pm) = payment_method {
                request_obj.insert("paymentMethod".to_string(), serde_json::json!(pm));
            }
            if let Some(pm_id) = payment_method_id {
                request_obj.insert("paymentMethodId".to_string(), serde_json::json!(pm_id));
            }

            // Validate required fields
            if !request_obj.contains_key("name") {
                return Err(RedisCtlError::InvalidInput {
                    message: "--name is required (or provide via --data JSON)".to_string(),
                });
            }
            if !request_obj.contains_key("planId") {
                return Err(RedisCtlError::InvalidInput {
                    message: "--plan-id is required (or provide via --data JSON)".to_string(),
                });
            }

            let request: FixedSubscriptionCreateRequest = serde_json::from_value(request_value)
                .context("Invalid subscription configuration")?;

            let result = handler
                .create(&request)
                .await
                .context("Failed to create fixed subscription")?;

            let json_result =
                serde_json::to_value(&result).context("Failed to serialize response")?;

            handle_async_response(
                conn_mgr,
                profile_name,
                json_result,
                async_ops,
                output_format,
                query,
                "Fixed subscription created successfully",
            )
            .await
        }

        CloudFixedSubscriptionCommands::Update {
            id,
            name,
            plan_id,
            payment_method,
            payment_method_id,
            data,
            async_ops,
        } => {
            // Start with data from --data if provided, otherwise empty object
            let mut request_value = if let Some(data_str) = data {
                read_json_data(data_str)?
            } else {
                serde_json::json!({})
            };

            let request_obj =
                request_value
                    .as_object_mut()
                    .ok_or_else(|| RedisCtlError::InvalidInput {
                        message: "JSON data must be an object".to_string(),
                    })?;

            // Apply first-class params (override JSON values)
            if let Some(n) = name {
                request_obj.insert("name".to_string(), serde_json::json!(n));
            }
            if let Some(p) = plan_id {
                request_obj.insert("planId".to_string(), serde_json::json!(p));
            }
            if let Some(pm) = payment_method {
                request_obj.insert("paymentMethod".to_string(), serde_json::json!(pm));
            }
            if let Some(pm_id) = payment_method_id {
                request_obj.insert("paymentMethodId".to_string(), serde_json::json!(pm_id));
            }

            // Validate that we have at least one field to update
            if request_obj.is_empty() {
                return Err(RedisCtlError::InvalidInput {
                    message: "At least one update field is required (--name, --plan-id, --payment-method, --payment-method-id, or --data)".to_string(),
                });
            }

            let request: FixedSubscriptionUpdateRequest =
                serde_json::from_value(request_value).context("Invalid update configuration")?;

            let result = handler
                .update(*id, &request)
                .await
                .context("Failed to update fixed subscription")?;

            let json_result =
                serde_json::to_value(&result).context("Failed to serialize response")?;

            handle_async_response(
                conn_mgr,
                profile_name,
                json_result,
                async_ops,
                output_format,
                query,
                "Fixed subscription updated successfully",
            )
            .await
        }

        CloudFixedSubscriptionCommands::Delete {
            id,
            force,
            async_ops,
        } => {
            if !force {
                let prompt = format!("Delete fixed subscription {}?", id);
                confirm_action(&prompt)?;
            }

            let result = handler
                .delete_by_id(*id)
                .await
                .context("Failed to delete fixed subscription")?;

            let json_result =
                serde_json::to_value(&result).context("Failed to serialize response")?;

            handle_async_response(
                conn_mgr,
                profile_name,
                json_result,
                async_ops,
                output_format,
                query,
                "Fixed subscription deleted successfully",
            )
            .await
        }

        CloudFixedSubscriptionCommands::RedisVersions { subscription } => {
            let versions = handler
                .get_redis_versions(*subscription)
                .await
                .context("Failed to get Redis versions")?;

            let json_response =
                serde_json::to_value(versions).context("Failed to serialize response")?;
            let data = handle_output(json_response, output_format, query)?;
            print_formatted_output(data, output_format)?;
            Ok(())
        }
    }
}

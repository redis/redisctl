use crate::cli::{CloudProviderAccountCommands, OutputFormat};
use crate::commands::cloud::cloud_account_impl::{
    self, CloudAccountOperationParams, CreateParams, UpdateParams,
};
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;

pub async fn handle_cloud_account_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &CloudProviderAccountCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;

    match command {
        CloudProviderAccountCommands::List => {
            cloud_account_impl::handle_list(&client, output_format, query).await
        }
        CloudProviderAccountCommands::Get { account_id } => {
            cloud_account_impl::handle_get(&client, *account_id, output_format, query).await
        }
        CloudProviderAccountCommands::Create {
            name,
            provider,
            access_key_id,
            access_secret_key,
            console_username,
            console_password,
            sign_in_login_url,
            data,
            async_ops,
        } => {
            let params = CloudAccountOperationParams {
                conn_mgr,
                profile_name,
                client: &client,
                async_ops,
                output_format,
                query,
            };
            let create_params = CreateParams {
                name: name.as_deref(),
                provider: provider.as_deref(),
                access_key_id: access_key_id.as_deref(),
                access_secret_key: access_secret_key.as_deref(),
                console_username: console_username.as_deref(),
                console_password: console_password.as_deref(),
                sign_in_login_url: sign_in_login_url.as_deref(),
                data: data.as_deref(),
            };
            cloud_account_impl::handle_create(&params, &create_params).await
        }
        CloudProviderAccountCommands::Update {
            account_id,
            name,
            access_key_id,
            access_secret_key,
            console_username,
            console_password,
            sign_in_login_url,
            data,
            async_ops,
        } => {
            let params = CloudAccountOperationParams {
                conn_mgr,
                profile_name,
                client: &client,
                async_ops,
                output_format,
                query,
            };
            let update_params = UpdateParams {
                name: name.as_deref(),
                access_key_id: access_key_id.as_deref(),
                access_secret_key: access_secret_key.as_deref(),
                console_username: console_username.as_deref(),
                console_password: console_password.as_deref(),
                sign_in_login_url: sign_in_login_url.as_deref(),
                data: data.as_deref(),
            };
            cloud_account_impl::handle_update(&params, *account_id, &update_params).await
        }
        CloudProviderAccountCommands::Delete {
            account_id,
            force,
            async_ops,
        } => {
            let params = CloudAccountOperationParams {
                conn_mgr,
                profile_name,
                client: &client,
                async_ops,
                output_format,
                query,
            };
            cloud_account_impl::handle_delete(&params, *account_id, *force).await
        }
    }
}

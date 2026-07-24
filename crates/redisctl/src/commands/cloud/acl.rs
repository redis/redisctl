use crate::cli::{CloudAclCommands, OutputFormat};
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;

use super::acl_impl::{self, AclOperationParams};

pub async fn handle_acl_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &CloudAclCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    match command {
        // Redis ACL Rules
        CloudAclCommands::ListRedisRules => {
            acl_impl::list_redis_rules(conn_mgr, profile_name, output_format, query).await
        }
        CloudAclCommands::CreateRedisRule {
            name,
            rule,
            async_ops,
        } => {
            let params = AclOperationParams {
                conn_mgr,
                profile_name,
                async_ops,
                output_format,
                query,
            };
            acl_impl::create_redis_rule(&params, name, rule).await
        }
        CloudAclCommands::UpdateRedisRule {
            id,
            name,
            rule,
            async_ops,
        } => {
            let params = AclOperationParams {
                conn_mgr,
                profile_name,
                async_ops,
                output_format,
                query,
            };
            acl_impl::update_redis_rule(&params, *id, name.as_deref(), rule.as_deref()).await
        }
        CloudAclCommands::DeleteRedisRule {
            id,
            force,
            async_ops,
        } => {
            let params = AclOperationParams {
                conn_mgr,
                profile_name,
                async_ops,
                output_format,
                query,
            };
            acl_impl::delete_redis_rule(&params, *id, *force).await
        }

        // ACL Roles
        CloudAclCommands::ListRoles => {
            acl_impl::list_roles(conn_mgr, profile_name, output_format, query).await
        }
        CloudAclCommands::CreateRole {
            name,
            redis_rules,
            async_ops,
        } => {
            let params = AclOperationParams {
                conn_mgr,
                profile_name,
                async_ops,
                output_format,
                query,
            };
            acl_impl::create_role(&params, name, redis_rules).await
        }
        CloudAclCommands::UpdateRole {
            id,
            name,
            redis_rules,
            async_ops,
        } => {
            let params = AclOperationParams {
                conn_mgr,
                profile_name,
                async_ops,
                output_format,
                query,
            };
            acl_impl::update_role(&params, *id, name.as_deref(), redis_rules.as_deref()).await
        }
        CloudAclCommands::DeleteRole {
            id,
            force,
            async_ops,
        } => {
            let params = AclOperationParams {
                conn_mgr,
                profile_name,
                async_ops,
                output_format,
                query,
            };
            acl_impl::delete_role(&params, *id, *force).await
        }

        // ACL Users
        CloudAclCommands::ListAclUsers => {
            acl_impl::list_acl_users(conn_mgr, profile_name, output_format, query).await
        }
        CloudAclCommands::GetAclUser { id } => {
            acl_impl::get_acl_user(conn_mgr, profile_name, *id, output_format, query).await
        }
        CloudAclCommands::CreateAclUser {
            name,
            role,
            password,
            async_ops,
        } => {
            let params = AclOperationParams {
                conn_mgr,
                profile_name,
                async_ops,
                output_format,
                query,
            };
            acl_impl::create_acl_user(&params, name, role, password).await
        }
        CloudAclCommands::UpdateAclUser {
            id,
            name,
            role,
            password,
            async_ops,
        } => {
            let params = AclOperationParams {
                conn_mgr,
                profile_name,
                async_ops,
                output_format,
                query,
            };
            acl_impl::update_acl_user(
                &params,
                *id,
                name.as_deref(),
                role.as_deref(),
                password.as_deref(),
            )
            .await
        }
        CloudAclCommands::DeleteAclUser {
            id,
            force,
            async_ops,
        } => {
            let params = AclOperationParams {
                conn_mgr,
                profile_name,
                async_ops,
                output_format,
                query,
            };
            acl_impl::delete_acl_user(&params, *id, *force).await
        }
    }
}

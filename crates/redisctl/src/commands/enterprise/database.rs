//! Enterprise database command handler

use crate::cli::{EnterpriseDatabaseCommands, OutputFormat};
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;

use super::database_impl;

/// Handle enterprise database commands
pub async fn handle_database_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &EnterpriseDatabaseCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    match command {
        EnterpriseDatabaseCommands::List => {
            database_impl::list_databases(conn_mgr, profile_name, output_format, query).await
        }
        EnterpriseDatabaseCommands::Get { id } => {
            database_impl::get_database(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseDatabaseCommands::Create {
            name,
            memory,
            port,
            replication,
            persistence,
            eviction_policy,
            sharding,
            shards_count,
            proxy_policy,
            crdb,
            redis_password,
            modules,
            data,
            dry_run,
        } => {
            database_impl::create_database(
                conn_mgr,
                profile_name,
                name.as_deref(),
                *memory,
                *port,
                *replication,
                persistence.as_deref(),
                eviction_policy.as_deref(),
                *sharding,
                *shards_count,
                proxy_policy.as_deref(),
                *crdb,
                redis_password.as_deref(),
                modules,
                data.as_deref(),
                *dry_run,
                output_format,
                query,
            )
            .await
        }
        EnterpriseDatabaseCommands::Update {
            id,
            name,
            memory,
            replication,
            persistence,
            eviction_policy,
            shards_count,
            proxy_policy,
            redis_password,
            data,
            dry_run,
        } => {
            database_impl::update_database(
                conn_mgr,
                profile_name,
                *id,
                name.as_deref(),
                *memory,
                *replication,
                persistence.as_deref(),
                eviction_policy.as_deref(),
                *shards_count,
                proxy_policy.as_deref(),
                redis_password.as_deref(),
                data.as_deref(),
                *dry_run,
                output_format,
                query,
            )
            .await
        }
        EnterpriseDatabaseCommands::Delete { id, force, dry_run } => {
            database_impl::delete_database(
                conn_mgr,
                profile_name,
                *id,
                *force,
                *dry_run,
                output_format,
                query,
            )
            .await
        }
        EnterpriseDatabaseCommands::Watch { id, poll_interval } => {
            database_impl::watch_database(conn_mgr, profile_name, *id, *poll_interval, query).await
        }
        EnterpriseDatabaseCommands::Export {
            id,
            location,
            aws_access_key,
            aws_secret_key,
            data,
        } => {
            database_impl::export_database(
                conn_mgr,
                profile_name,
                *id,
                location.as_deref(),
                aws_access_key.as_deref(),
                aws_secret_key.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseDatabaseCommands::Import {
            id,
            location,
            aws_access_key,
            aws_secret_key,
            flush,
            data,
            async_ops,
        } => {
            database_impl::import_database(
                conn_mgr,
                profile_name,
                *id,
                location.as_deref(),
                aws_access_key.as_deref(),
                aws_secret_key.as_deref(),
                *flush,
                data.as_deref(),
                async_ops,
                output_format,
                query,
            )
            .await
        }
        EnterpriseDatabaseCommands::Backup { id, async_ops } => {
            database_impl::backup_database(
                conn_mgr,
                profile_name,
                *id,
                async_ops,
                output_format,
                query,
            )
            .await
        }
        EnterpriseDatabaseCommands::Restore {
            id,
            backup_uid,
            data,
        } => {
            database_impl::restore_database(
                conn_mgr,
                profile_name,
                *id,
                backup_uid.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseDatabaseCommands::Flush { id, force } => {
            database_impl::flush_database(conn_mgr, profile_name, *id, *force, output_format, query)
                .await
        }
        EnterpriseDatabaseCommands::GetShards { id } => {
            database_impl::get_database_shards(conn_mgr, profile_name, *id, output_format, query)
                .await
        }
        EnterpriseDatabaseCommands::UpdateShards {
            id,
            shards_count,
            shards_placement,
            data,
        } => {
            database_impl::update_database_shards(
                conn_mgr,
                profile_name,
                *id,
                *shards_count,
                shards_placement.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseDatabaseCommands::GetModules { id } => {
            database_impl::get_database_modules(conn_mgr, profile_name, *id, output_format, query)
                .await
        }
        EnterpriseDatabaseCommands::UpdateModules {
            id,
            add_modules,
            remove_modules,
            data,
        } => {
            database_impl::update_database_modules(
                conn_mgr,
                profile_name,
                *id,
                add_modules,
                remove_modules,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseDatabaseCommands::Upgrade {
            id,
            version,
            preserve_roles,
            force_restart,
            may_discard_data,
            force_discard,
            keep_crdt_protocol_version,
            parallel_shards_upgrade,
            force,
        } => {
            database_impl::upgrade_database(
                conn_mgr,
                profile_name,
                *id,
                version.as_deref(),
                *preserve_roles,
                *force_restart,
                *may_discard_data,
                *force_discard,
                *keep_crdt_protocol_version,
                *parallel_shards_upgrade,
                *force,
                output_format,
                query,
            )
            .await
        }
        EnterpriseDatabaseCommands::GetAcl { id } => {
            database_impl::get_database_acl(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseDatabaseCommands::UpdateAcl {
            id,
            acl_uid,
            default_user,
            data,
        } => {
            database_impl::update_database_acl(
                conn_mgr,
                profile_name,
                *id,
                *acl_uid,
                *default_user,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseDatabaseCommands::Stats { id } => {
            database_impl::get_database_stats(conn_mgr, profile_name, *id, output_format, query)
                .await
        }
        EnterpriseDatabaseCommands::Metrics { id, interval } => {
            database_impl::get_database_metrics(
                conn_mgr,
                profile_name,
                *id,
                interval.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseDatabaseCommands::Slowlog { id, limit } => {
            database_impl::get_database_slowlog(
                conn_mgr,
                profile_name,
                *id,
                *limit,
                output_format,
                query,
            )
            .await
        }
        EnterpriseDatabaseCommands::ClientList { id } => {
            database_impl::get_database_clients(conn_mgr, profile_name, *id, output_format, query)
                .await
        }
    }
}

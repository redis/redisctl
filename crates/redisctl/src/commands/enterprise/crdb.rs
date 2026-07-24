use crate::cli::{EnterpriseCrdbCommands, OutputFormat};
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;

use super::crdb_impl;

pub async fn handle_crdb_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &EnterpriseCrdbCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    match command {
        // CRDB Lifecycle
        EnterpriseCrdbCommands::List => {
            crdb_impl::list_crdbs(conn_mgr, profile_name, output_format, query).await
        }
        EnterpriseCrdbCommands::Get { id } => {
            crdb_impl::get_crdb(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseCrdbCommands::Create {
            name,
            memory_size,
            default_db_name,
            encryption,
            data,
        } => {
            crdb_impl::create_crdb(
                conn_mgr,
                profile_name,
                name.as_deref(),
                *memory_size,
                default_db_name.as_deref(),
                *encryption,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseCrdbCommands::Update {
            id,
            memory_size,
            encryption,
            data_persistence,
            replication,
            eviction_policy,
            data,
        } => {
            crdb_impl::update_crdb(
                conn_mgr,
                profile_name,
                *id,
                *memory_size,
                *encryption,
                data_persistence.as_deref(),
                *replication,
                eviction_policy.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseCrdbCommands::Delete { id, force } => {
            crdb_impl::delete_crdb(conn_mgr, profile_name, *id, *force, output_format, query).await
        }

        // Participating Clusters
        EnterpriseCrdbCommands::GetClusters { id } => {
            crdb_impl::get_participating_clusters(conn_mgr, profile_name, *id, output_format, query)
                .await
        }
        EnterpriseCrdbCommands::AddCluster {
            id,
            url,
            name,
            username,
            password,
            compression,
            data,
        } => {
            crdb_impl::add_cluster_to_crdb(
                conn_mgr,
                profile_name,
                *id,
                url.as_deref(),
                name.as_deref(),
                username.as_deref(),
                password.as_deref(),
                *compression,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseCrdbCommands::RemoveCluster { id, cluster } => {
            crdb_impl::remove_cluster_from_crdb(
                conn_mgr,
                profile_name,
                *id,
                *cluster,
                false,
                output_format,
                query,
            )
            .await
        }
        EnterpriseCrdbCommands::UpdateCluster {
            id,
            cluster,
            url,
            compression,
            proxy_policy,
            data,
        } => {
            crdb_impl::update_cluster_in_crdb(
                conn_mgr,
                profile_name,
                *id,
                *cluster,
                url.as_deref(),
                *compression,
                proxy_policy.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }

        // Instance Management
        EnterpriseCrdbCommands::GetInstances { id } => {
            crdb_impl::get_crdb_instances(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseCrdbCommands::GetInstance { crdb_id, instance } => {
            crdb_impl::get_crdb_instance(
                conn_mgr,
                profile_name,
                *crdb_id,
                *instance,
                output_format,
                query,
            )
            .await
        }
        EnterpriseCrdbCommands::UpdateInstance {
            crdb_id,
            instance,
            memory_size,
            port,
            enabled,
            data,
        } => {
            crdb_impl::update_crdb_instance(
                conn_mgr,
                profile_name,
                *crdb_id,
                *instance,
                *memory_size,
                *port,
                *enabled,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseCrdbCommands::FlushInstance {
            crdb_id,
            instance,
            force,
        } => {
            crdb_impl::flush_crdb_instance(
                conn_mgr,
                profile_name,
                *crdb_id,
                *instance,
                *force,
                output_format,
                query,
            )
            .await
        }

        // Replication & Sync
        EnterpriseCrdbCommands::GetReplicationStatus { id } => {
            crdb_impl::get_replication_status(conn_mgr, profile_name, *id, output_format, query)
                .await
        }
        EnterpriseCrdbCommands::GetLag { id } => {
            crdb_impl::get_replication_lag(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseCrdbCommands::ForceSync { id, source } => {
            crdb_impl::force_sync_crdb(conn_mgr, profile_name, *id, *source, output_format, query)
                .await
        }
        EnterpriseCrdbCommands::PauseReplication { id } => {
            crdb_impl::pause_replication(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseCrdbCommands::ResumeReplication { id } => {
            crdb_impl::resume_replication(conn_mgr, profile_name, *id, output_format, query).await
        }

        // Conflict Resolution
        EnterpriseCrdbCommands::GetConflicts { id, limit } => {
            crdb_impl::get_conflicts(conn_mgr, profile_name, *id, *limit, output_format, query)
                .await
        }
        EnterpriseCrdbCommands::GetConflictPolicy { id } => {
            crdb_impl::get_conflict_policy(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseCrdbCommands::UpdateConflictPolicy {
            id,
            policy,
            source_id,
            data,
        } => {
            crdb_impl::update_conflict_policy(
                conn_mgr,
                profile_name,
                *id,
                policy.as_deref(),
                *source_id,
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseCrdbCommands::ResolveConflict {
            id,
            conflict,
            resolution,
        } => {
            crdb_impl::resolve_conflict(
                conn_mgr,
                profile_name,
                *id,
                conflict,
                resolution,
                output_format,
                query,
            )
            .await
        }

        // Tasks & Jobs
        EnterpriseCrdbCommands::GetTasks { id } => {
            crdb_impl::get_crdb_tasks(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseCrdbCommands::GetTask { crdb_id, task } => {
            crdb_impl::get_crdb_task(
                conn_mgr,
                profile_name,
                *crdb_id,
                task.clone(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseCrdbCommands::RetryTask { crdb_id, task } => {
            crdb_impl::retry_crdb_task(
                conn_mgr,
                profile_name,
                *crdb_id,
                task.clone(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseCrdbCommands::CancelTask { crdb_id, task } => {
            crdb_impl::cancel_crdb_task(
                conn_mgr,
                profile_name,
                *crdb_id,
                task.clone(),
                output_format,
                query,
            )
            .await
        }

        // Monitoring & Metrics
        EnterpriseCrdbCommands::Stats { id } => {
            crdb_impl::get_crdb_stats(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseCrdbCommands::Metrics { id, interval } => {
            crdb_impl::get_crdb_metrics(
                conn_mgr,
                profile_name,
                *id,
                interval.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseCrdbCommands::GetConnections { id } => {
            crdb_impl::get_crdb_connections(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseCrdbCommands::GetThroughput { id } => {
            crdb_impl::get_crdb_throughput(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseCrdbCommands::HealthCheck { id } => {
            crdb_impl::health_check_crdb(conn_mgr, profile_name, *id, output_format, query).await
        }

        // Backup & Recovery
        EnterpriseCrdbCommands::Backup { id, location, data } => {
            crdb_impl::backup_crdb(
                conn_mgr,
                profile_name,
                *id,
                location.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseCrdbCommands::Restore {
            id,
            backup_uid,
            location,
            data,
        } => {
            crdb_impl::restore_crdb(
                conn_mgr,
                profile_name,
                *id,
                backup_uid.as_deref(),
                location.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        EnterpriseCrdbCommands::GetBackups { id } => {
            crdb_impl::get_crdb_backups(conn_mgr, profile_name, *id, output_format, query).await
        }
        EnterpriseCrdbCommands::Export { id, location, data } => {
            crdb_impl::export_crdb(
                conn_mgr,
                profile_name,
                *id,
                location.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
    }
}

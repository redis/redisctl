---
name: redisctl-enterprise-admin
description: Advanced Redis Enterprise administration via the redisctl CLI. Use for RBAC, LDAP, cluster policy, licensing, certificates, proxy management, and diagnostics.
---

## Overview

Advanced cluster administration tasks: access control, security, licensing, maintenance, and troubleshooting.

## RBAC (Role-Based Access Control)

### Users

```bash
redisctl enterprise user list
redisctl enterprise user get 1
redisctl enterprise user create --data '{...}'
redisctl enterprise user update 1 --data '{...}'
redisctl enterprise user delete 1
```

### Roles

```bash
redisctl enterprise role list
redisctl enterprise role get 1
redisctl enterprise role create --data '{...}'
redisctl enterprise role update 1 --data '{...}'
redisctl enterprise role delete 1
```

### ACLs

```bash
redisctl enterprise acl list
redisctl enterprise acl get 1
redisctl enterprise acl create --data '{...}'
redisctl enterprise acl update 1 --data '{...}'
redisctl enterprise acl delete 1
```

## LDAP Integration

```bash
# Get LDAP configuration
redisctl enterprise ldap get

# Update LDAP configuration
redisctl enterprise ldap update --data '{...}'

# Test LDAP connectivity
redisctl enterprise ldap test

# Manage LDAP role mappings
redisctl enterprise ldap-mappings list
redisctl enterprise ldap-mappings create --data '{...}'
redisctl enterprise ldap-mappings update 1 --data '{...}'
redisctl enterprise ldap-mappings delete 1
```

## Cluster Configuration

```bash
# Get cluster config
redisctl enterprise cluster get

# Update cluster config
redisctl enterprise cluster update --data '{...}'

# Get/update cluster policies
redisctl enterprise cluster get-policy
redisctl enterprise cluster update-policy --data '{...}'
```

## Maintenance Mode

```bash
# Enable maintenance mode (prevents automatic failover)
redisctl enterprise cluster enable-maintenance-mode

# Disable maintenance mode
redisctl enterprise cluster disable-maintenance-mode
```

## Licensing

```bash
# Get current license info
redisctl enterprise license get

# Upload a license file
redisctl enterprise license upload --file license.key

# Update license with a key string or JSON data
redisctl enterprise license update --license-key <KEY>
redisctl enterprise license update --data @license.json

# Validate, check expiry, and view licensed features/usage
redisctl enterprise license validate
redisctl enterprise license expiry
redisctl enterprise license features
redisctl enterprise license usage
```

## Proxy Management

```bash
redisctl enterprise proxy list
redisctl enterprise proxy get <UID>
redisctl enterprise proxy update <UID> --data '{...}'
```

## Service Management

```bash
redisctl enterprise services list
redisctl enterprise services get --name <service>
redisctl enterprise services update --name <service> --data '{...}'
```

## Diagnostics

```bash
# Get / update diagnostics configuration
redisctl enterprise diagnostics get
redisctl enterprise diagnostics update --data '{...}'

# Run diagnostic checks
redisctl enterprise diagnostics run

# List available checks and reports
redisctl enterprise diagnostics list-checks
redisctl enterprise diagnostics list-reports

# Get the last or a specific diagnostic report
redisctl enterprise diagnostics last-report
redisctl enterprise diagnostics get-report <ID>

# Support package for Redis support
redisctl enterprise support-package cluster
redisctl enterprise support-package database
redisctl enterprise support-package node

# Check status of an async support-package generation task
redisctl enterprise support-package status <task-id>

# List previously generated support packages
redisctl enterprise support-package list
```

## Tips

- Maintenance mode should be enabled before planned node maintenance to prevent automatic failover
- Always test LDAP configuration with `ldap test` before applying to production
- Support packages contain sensitive data -- handle with care
- License operations may require cluster restart for some changes

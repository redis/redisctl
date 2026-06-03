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
redisctl enterprise user get --id 1
redisctl enterprise user create --data '{...}'
redisctl enterprise user update --id 1 --data '{...}'
redisctl enterprise user delete --id 1
```

### Roles

```bash
redisctl enterprise role list
redisctl enterprise role get --id 1
redisctl enterprise role create --data '{...}'
redisctl enterprise role update --id 1 --data '{...}'
redisctl enterprise role delete --id 1
```

### ACLs

```bash
redisctl enterprise acl list
redisctl enterprise acl get --id 1
redisctl enterprise acl create --data '{...}'
redisctl enterprise acl update --id 1 --data '{...}'
redisctl enterprise acl delete --id 1
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
redisctl enterprise ldap-mappings update --id 1 --data '{...}'
redisctl enterprise ldap-mappings delete --id 1
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

# Update license
redisctl enterprise license update --file license.key
```

## Proxy Management

```bash
redisctl enterprise proxy list
redisctl enterprise proxy get --id 1
redisctl enterprise proxy update --id 1 --data '{...}'
```

## Service Management

```bash
redisctl enterprise services list
redisctl enterprise services get --name <service>
redisctl enterprise services update --name <service> --data '{...}'
```

## Diagnostics

```bash
# Cluster diagnostics
redisctl enterprise diagnostics cluster

# Node diagnostics
redisctl enterprise diagnostics node

# Debug info collection
redisctl enterprise debug-info

# Support package for Redis support
redisctl enterprise support-package create
redisctl enterprise support-package status --id <pkg-id>
redisctl enterprise support-package download --id <pkg-id>
```

## Tips

- Maintenance mode should be enabled before planned node maintenance to prevent automatic failover
- Always test LDAP configuration with `ldap test` before applying to production
- Support packages contain sensitive data -- handle with care
- License operations may require cluster restart for some changes

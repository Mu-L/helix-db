# Helix CLI v2 Migration Guide

## Breaking Changes: `helix add` and `helix init` Commands

### Old Syntax (v1.x)
```bash
# Add instances with subcommands
helix add myapp                  # Local instance
helix add myapp fly              # Fly.io instance
helix add myapp ecr              # ECR instance
helix add myapp helix            # Helix cloud instance

# Fly.io with options
helix add myapp fly --auth cli --volume-size 50 --vm-size shared-cpu-4x --public true

# Initialize with deployment type
helix init
helix init fly
helix init ecr
```

### New Syntax (v2.0)
```bash
# Add instances with flags
helix add myapp                  # Local instance (default)
helix add myapp --fly            # Fly.io instance
helix add myapp --ecr            # ECR instance
helix add myapp --cloud          # Helix cloud instance

# Cloud instances with region
helix add myapp --cloud --cloud-region eu-west-1

# Fly.io with options
helix add myapp --fly --fly-volume-size 50 --fly-vm-size shared-cpu-4x --fly-public

# Initialize with deployment type
helix init                       # Local instance (default)
helix init --fly                 # With Fly.io instance
helix init --ecr                 # With ECR instance
helix init --cloud               # With Helix cloud instance
helix init --cloud --cloud-region us-west-2  # With specific region
```

## Key Changes

1. **Subcommands → Flags**: Instance types are now specified with flags instead of subcommands
2. **Provider-prefixed options**: Fly.io options now use `--fly-` prefix for clarity
3. **Cloud region support**: New `--cloud-region` flag for Helix cloud instances
4. **Helix cloud implementation**: Fully implemented cloud instance creation with configuration management
5. **Mutually exclusive**: Only one instance type flag can be specified at a time
6. **Better discoverability**: All options are visible in `--help`

## Benefits

- More intuitive CLI interface
- All options visible in help text
- Consistent with common CLI patterns
- Clear provider-specific option grouping
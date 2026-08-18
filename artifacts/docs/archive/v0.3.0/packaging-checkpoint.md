# Packaging Checkpoint Helper

The development packaging helper is:

```powershell
.\artifacts\dev\wtg_package_checkpoint.ps1
```

Default behavior:

- derives the package label from the current branch when `-Label` is omitted
- debug build unless `-Release` is supplied
- output under `artifacts\packages`

Useful options:

```powershell
.\artifacts\dev\wtg_package_checkpoint.ps1 -Label probe-fields
.\artifacts\dev\wtg_package_checkpoint.ps1 -Release
.\artifacts\dev\wtg_package_checkpoint.ps1 -Release -CleanPackages
```

The helper captures git/build metadata and CLI validation outputs. On branches that produce `wtg-ui.exe`, it includes and hashes the UI binary but does not launch it.

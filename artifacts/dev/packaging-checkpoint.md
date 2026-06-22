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

`-CleanPackages` refreshes `artifacts\packages` while preserving `.gitkeep`.

The checkpoint package captures git/build metadata and CLI validation outputs from `wtg.exe`.

On branches that produce `wtg-ui.exe`, the helper passively includes and hashes the UI binary, but it does not launch the UI.

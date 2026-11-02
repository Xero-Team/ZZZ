# The lint gate lives in `cargo xtask clippy`; this shim keeps the Windows
# entry point that older docs reference working.
$ErrorActionPreference = "Stop"

cargo xtask clippy @args
exit $LASTEXITCODE

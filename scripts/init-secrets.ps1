# Generate installation-specific secrets outside the Git working tree.

[CmdletBinding()]
param(
    [string]$EnvironmentFile
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($EnvironmentFile)) {
    $EnvironmentFile = Join-Path $RepoRoot ".env"
}
$EnvironmentExample = Join-Path $RepoRoot ".env.example"
$Utf8NoBom = New-Object System.Text.UTF8Encoding($false)

function Get-EnvironmentValue {
    param([string]$Name, [string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return ""
    }
    $prefix = "$Name="
    $match = Get-Content -LiteralPath $Path |
        Where-Object { $_.StartsWith($prefix, [System.StringComparison]::Ordinal) } |
        Select-Object -Last 1
    if ($null -eq $match) {
        return ""
    }
    return $match.Substring($prefix.Length)
}

function New-RandomHex {
    param([int]$ByteCount)
    $bytes = New-Object byte[] $ByteCount
    $generator = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    try {
        $generator.GetBytes($bytes)
    }
    finally {
        $generator.Dispose()
    }
    return (($bytes | ForEach-Object { $_.ToString("x2") }) -join "")
}

function Test-PrivateIPv4 {
    param([System.Net.IPAddress]$Address)
    if ($null -eq $Address -or
        $Address.AddressFamily -ne [System.Net.Sockets.AddressFamily]::InterNetwork) {
        return $false
    }
    $bytes = $Address.GetAddressBytes()
    return $bytes[0] -eq 10 -or
        ($bytes[0] -eq 172 -and $bytes[1] -ge 16 -and $bytes[1] -le 31) -or
        ($bytes[0] -eq 192 -and $bytes[1] -eq 168)
}

function Get-PrivateLanIPv4 {
    if (Get-Command Get-NetRoute -ErrorAction SilentlyContinue) {
        $routes = Get-NetRoute -AddressFamily IPv4 -DestinationPrefix "0.0.0.0/0" `
            -ErrorAction SilentlyContinue |
            Sort-Object RouteMetric, InterfaceMetric
        foreach ($route in $routes) {
            $addresses = Get-NetIPAddress -AddressFamily IPv4 `
                -InterfaceIndex $route.InterfaceIndex -ErrorAction SilentlyContinue |
                Sort-Object SkipAsSource
            foreach ($entry in $addresses) {
                if (Test-PrivateIPv4 -Address $entry.IPAddress) {
                    return $entry.IPAddress.ToString()
                }
            }
        }
    }

    $interfaces = [System.Net.NetworkInformation.NetworkInterface]::GetAllNetworkInterfaces() |
        Where-Object {
            $_.OperationalStatus -eq [System.Net.NetworkInformation.OperationalStatus]::Up -and
            $_.NetworkInterfaceType -ne [System.Net.NetworkInformation.NetworkInterfaceType]::Loopback -and
            $_.NetworkInterfaceType -ne [System.Net.NetworkInformation.NetworkInterfaceType]::Tunnel
        } |
        Sort-Object Speed -Descending
    foreach ($interface in $interfaces) {
        $properties = $interface.GetIPProperties()
        $hasIPv4Gateway = $properties.GatewayAddresses |
            Where-Object {
                $_.Address.AddressFamily -eq [System.Net.Sockets.AddressFamily]::InterNetwork -and
                -not $_.Address.Equals([System.Net.IPAddress]::Any)
            } |
            Select-Object -First 1
        if ($null -eq $hasIPv4Gateway) {
            continue
        }
        foreach ($entry in $properties.UnicastAddresses) {
            if (Test-PrivateIPv4 -Address $entry.Address) {
                return $entry.Address.ToString()
            }
        }
    }

    return "127.0.0.1"
}

function Set-SecretIfMissing {
    param(
        [string]$Path,
        [string]$LegacyValue,
        [int]$ByteCount,
        [int]$MinimumLength
    )
    if ((Test-Path -LiteralPath $Path) -and
        (Get-Item -LiteralPath $Path).Length -gt 0) {
        return
    }

    $value = $LegacyValue
    if ([string]::IsNullOrWhiteSpace($value) -or
        $value.StartsWith("replace-with-", [System.StringComparison]::Ordinal)) {
        $value = New-RandomHex -ByteCount $ByteCount
    }
    if ($value.Length -lt $MinimumLength -or $value -notmatch "^[A-Za-z0-9_]+$") {
        throw "Legacy secret is invalid."
    }
    [System.IO.File]::WriteAllText($Path, "$value`n", $Utf8NoBom)
    $value = $null
}

$existingSecretDirectory = Get-EnvironmentValue `
    -Name "OPENNODIA_SECRETS_DIR" `
    -Path $EnvironmentFile
if (-not [string]::IsNullOrWhiteSpace($env:OPENNODIA_SECRETS_DIR)) {
    $SecretDirectory = $env:OPENNODIA_SECRETS_DIR
}
elseif (-not [string]::IsNullOrWhiteSpace($existingSecretDirectory)) {
    $SecretDirectory = $existingSecretDirectory
}
else {
    $configHome = $env:XDG_CONFIG_HOME
    if ([string]::IsNullOrWhiteSpace($configHome)) {
        $configHome = Join-Path $HOME ".config"
    }
    $SecretDirectory = Join-Path $configHome "opennodia\secrets"
}
$SecretDirectory = [System.IO.Path]::GetFullPath($SecretDirectory)

$legacyAlgodToken = Get-EnvironmentValue `
    -Name "ALGOD_TOKEN" `
    -Path $EnvironmentFile
$legacyDatabasePassword = Get-EnvironmentValue `
    -Name "INDEXER_DB_PASSWORD" `
    -Path $EnvironmentFile
$existingBindAddress = Get-EnvironmentValue `
    -Name "OPENNODIA_BIND_ADDRESS" `
    -Path $EnvironmentFile
if (-not [string]::IsNullOrWhiteSpace($env:OPENNODIA_BIND_ADDRESS)) {
    $BindAddress = $env:OPENNODIA_BIND_ADDRESS
}
elseif (-not [string]::IsNullOrWhiteSpace($existingBindAddress)) {
    $BindAddress = $existingBindAddress
}
else {
    $BindAddress = Get-PrivateLanIPv4
    if ($BindAddress -eq "127.0.0.1") {
        Write-Warning "No private LAN IPv4 address detected; using loopback."
    }
}

New-Item -ItemType Directory -Force -Path $SecretDirectory | Out-Null
Set-SecretIfMissing `
    -Path (Join-Path $SecretDirectory "algod.token") `
    -LegacyValue $legacyAlgodToken `
    -ByteCount 32 `
    -MinimumLength 64
Set-SecretIfMissing `
    -Path (Join-Path $SecretDirectory "indexer-db-password") `
    -LegacyValue $legacyDatabasePassword `
    -ByteCount 24 `
    -MinimumLength 24
$legacyAlgodToken = $null
$legacyDatabasePassword = $null

if (Get-Command icacls.exe -ErrorAction SilentlyContinue) {
    $identity = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
    $hasNativePreference = Test-Path variable:PSNativeCommandUseErrorActionPreference
    if ($hasNativePreference) {
        $previousNativePreference = $PSNativeCommandUseErrorActionPreference
        $PSNativeCommandUseErrorActionPreference = $false
    }
    try {
        $aclOutput = & icacls.exe $SecretDirectory /inheritance:r /grant:r "${identity}:(OI)(CI)F" /T 2>&1
        $aclExitCode = $LASTEXITCODE
    }
    finally {
        if ($hasNativePreference) {
            $PSNativeCommandUseErrorActionPreference = $previousNativePreference
        }
    }
    if ($aclExitCode -ne 0) {
        $message = "Could not restrict secret directory ACLs with icacls: $aclOutput"
        if ($env:OPENNODIA_REQUIRE_STRICT_ACL -eq "true") {
            throw $message
        }
        Write-Warning $message
        $global:LASTEXITCODE = 0
    }
}

if (-not (Test-Path -LiteralPath $EnvironmentFile)) {
    Copy-Item -LiteralPath $EnvironmentExample -Destination $EnvironmentFile
}
$lines = Get-Content -LiteralPath $EnvironmentFile |
    Where-Object {
        $_ -notmatch "^(ALGOD_TOKEN|INDEXER_DB_PASSWORD|OPENNODIA_SECRETS_DIR|OPENNODIA_BIND_ADDRESS)="
    }
$composeSecretDirectory = $SecretDirectory.Replace("\", "/")
$updated = @($lines)
$updated += ""
$updated += "OPENNODIA_BIND_ADDRESS=$BindAddress"
$updated += "OPENNODIA_SECRETS_DIR=$composeSecretDirectory"
[System.IO.File]::WriteAllLines($EnvironmentFile, $updated, $Utf8NoBom)

if (Get-Command git -ErrorAction SilentlyContinue) {
    & git -C $RepoRoot config core.hooksPath .githooks
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "Could not configure Git hooks for this checkout."
        $global:LASTEXITCODE = 0
    }
}

Write-Host "OpenNodia secrets initialized outside the repository."
Write-Host "Secret directory: $SecretDirectory"
Write-Host "Docker Compose environment: $EnvironmentFile"
$HostPort = $env:OPENNODIA_HOST_PORT
if ([string]::IsNullOrWhiteSpace($HostPort)) {
    $HostPort = "30080"
}
Write-Host "Web UI: http://${BindAddress}:${HostPort}"

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$stage = Join-Path $env:ProgramFiles ('EasyRazer.DriverInstall.' + [Guid]::NewGuid().ToString('N'))

function Invoke-Tool([string] $file, [string[]] $arguments) {
    $output = & $file @arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw "$file failed ($LASTEXITCODE): $output" }
}

try {
    New-Item -ItemType Directory -Path $stage | Out-Null
    $zip = Join-Path $stage 'HIDMaestro.zip'
    Invoke-WebRequest -UseBasicParsing -TimeoutSec 300 -Uri 'https://github.com/hifihedgehog/HIDMaestro/releases/download/v1.12.0/HIDMaestro-v1.12.0.zip' -OutFile $zip
    if ((Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash -ne 'CF03347A0BC88BFB198A606C0FF8375BC4A9420FA80C07EF336186986D79B244') {
        throw 'HIDMaestro release checksum mismatch'
    }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($zip)
    try {
        $entry = $archive.Entries | Where-Object Name -eq 'HIDMaestro.Core.dll' | Select-Object -First 1
        if (-not $entry) { throw 'HIDMaestro SDK is missing from the release' }
        $sdk = Join-Path $stage 'HIDMaestro.Core.dll'
        [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $sdk)
    } finally { $archive.Dispose() }
    $assembly = [Reflection.Assembly]::Load([IO.File]::ReadAllBytes($sdk))
    $neutral = @('Inf2Cat.exe', 'inf2cat.exe.manifest', 'WindowsProtectedFiles.xml', 'Microsoft.UniversalStore.HardwareWorkflow.Cabinets.dll', 'Microsoft.UniversalStore.HardwareWorkflow.Catalogs.dll', 'Microsoft.UniversalStore.HardwareWorkflow.InfReader.dll', 'Microsoft.UniversalStore.HardwareWorkflow.SubmissionBuilder.dll', 'Microsoft.Kits.Logger.dll')
    foreach ($name in $assembly.GetManifestResourceNames()) {
        if ($name.StartsWith('HIDMaestro.Native.x64.')) { $file = $name.Substring(22) }
        elseif ($name.StartsWith('HIDMaestro.Resources.') -and $neutral -contains $name.Substring(21)) { $file = $name.Substring(21) }
        else { continue }
        if ([IO.Path]::GetFileName($file) -ne $file) { throw 'Invalid payload filename' }
        $source = $assembly.GetManifestResourceStream($name)
        $target = [IO.File]::Create((Join-Path $stage $file))
        try { $source.CopyTo($target) } finally { $target.Dispose(); $source.Dispose() }
    }
    $cert = Get-ChildItem Cert:\LocalMachine\My | Where-Object {
        $_.Subject -eq 'CN=HIDMaestroTestCert' -and $_.HasPrivateKey -and $_.NotAfter -gt (Get-Date).AddDays(1)
    } | Select-Object -First 1
    if (-not $cert) {
        $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=HIDMaestroTestCert' -CertStoreLocation Cert:\LocalMachine\My -NotAfter (Get-Date).AddYears(10)
    }
    foreach ($name in @('Root', 'TrustedPublisher')) {
        $store = New-Object Security.Cryptography.X509Certificates.X509Store($name, 'LocalMachine')
        $store.Open('ReadWrite')
        try { $store.Add($cert) } finally { $store.Close() }
    }
    $sign = Join-Path $stage 'signtool.exe'
    foreach ($file in @('HIDMaestro.dll', 'HMXInput.dll')) {
        Invoke-Tool $sign @('sign', '/sm', '/s', 'My', '/sha1', $cert.Thumbprint, '/fd', 'SHA256', (Join-Path $stage $file))
    }
    Invoke-Tool (Join-Path $stage 'Inf2Cat.exe') @("/driver:$stage", '/os:10_X64')
    foreach ($file in @('hidmaestro.cat', 'hidmaestro_xusb.cat')) {
        Invoke-Tool $sign @('sign', '/sm', '/s', 'My', '/sha1', $cert.Thumbprint, '/fd', 'SHA256', (Join-Path $stage $file))
    }
    foreach ($file in @('hidmaestro.inf', 'hidmaestro_xusb.inf')) {
        $output = & "$env:SystemRoot\System32\pnputil.exe" /add-driver (Join-Path $stage $file) /install 2>&1
        if ($LASTEXITCODE -notin @(0, 3010)) { throw "Driver installation failed ($LASTEXITCODE): $output" }
    }
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
} finally {
    if (Test-Path -LiteralPath $stage) {
        Get-ChildItem -LiteralPath $stage -File | Remove-Item -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $stage -ErrorAction SilentlyContinue
    }
}
exit 0

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
# Not $env:ProgramFiles: this runs elevated, and the user's environment could point it elsewhere.
$base = [Environment]::GetFolderPath('ProgramFiles')
$stage = Join-Path $base ('EasyRazer.DriverInstall.' + [Guid]::NewGuid().ToString('N'))

try {
    New-Item -ItemType Directory -Path $stage | Out-Null
    $exe = Join-Path $stage 'hidshim.exe'
    Invoke-WebRequest -UseBasicParsing -TimeoutSec 300 -Uri 'https://github.com/janeblower/hidshim/releases/download/0.1.0/hidshim.exe' -OutFile $exe
    if ((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -ne 'B9E84BEA2864207E7463C3C6EC1C2BEAA38838FFC5B32FD8198A7205A2D3E268') {
        throw 'hidshim release checksum mismatch'
    }
    # hidshim copies itself to Program Files and lists itself among installed apps.
    $output = & $exe install 2>&1
    if ($LASTEXITCODE -ne 0) { throw "hidshim installation failed ($LASTEXITCODE): $output" }
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

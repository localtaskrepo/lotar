# Install the LoTaR CLI on Windows from GitHub Releases (DEV-250).
#
#   irm https://localtaskrepo.github.io/lotar/install.ps1 | iex
#
# Options come from environment variables, since piped scripts take no arguments:
#   $env:LOTAR_VERSION      release to install, e.g. v0.11.0 (default: latest)
#   $env:LOTAR_INSTALL_DIR  where to put lotar.exe (default: %LOCALAPPDATA%\Programs\lotar)
#
# The archive is checked against the release's SHA-256 file before anything is
# installed, and the install directory is added to your user PATH. Releases are
# also signed with Sigstore; see https://github.com/localtaskrepo/lotar/blob/main/docs/help/install.md

function Install-Lotar {
    param(
        [string]$Version = $env:LOTAR_VERSION,
        [string]$InstallDir = $env:LOTAR_INSTALL_DIR,
        # Override only for mirrors or tests; the layout must match GitHub Releases.
        [string]$Releases = $(if ($env:LOTAR_RELEASES_URL) { $env:LOTAR_RELEASES_URL } else { 'https://github.com/localtaskrepo/lotar/releases' })
    )

    $ErrorActionPreference = 'Stop'
    # Invoke-WebRequest is much slower with the progress bar on Windows PowerShell.
    $ProgressPreference = 'SilentlyContinue'
    # Windows PowerShell 5.1 may not offer TLS 1.2 by default.
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $cpu = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    $platform = switch ($cpu) {
        'AMD64' { 'windows-x64' }
        'ARM64' { 'windows-arm64' }
        default { throw "Unsupported CPU '$cpu'. See $Releases for the available builds." }
    }

    if (-not $InstallDir) {
        if (-not $env:LOCALAPPDATA) { throw 'LOCALAPPDATA is not set; set $env:LOTAR_INSTALL_DIR.' }
        $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\lotar'
    }

    if (-not $Version) {
        # The /latest page redirects to /tag/<version>, which avoids GitHub API rate limits.
        $response = Invoke-WebRequest -Uri "$Releases/latest" -UseBasicParsing
        $final = if ($response.BaseResponse.ResponseUri) { $response.BaseResponse.ResponseUri } else { $response.BaseResponse.RequestMessage.RequestUri }
        $Version = ([string]$final).TrimEnd('/').Split('/')[-1]
        if ($Version -notmatch '^v\d') { throw "Could not find the latest release at $Releases/latest." }
    }
    if (-not $Version.StartsWith('v')) { $Version = "v$Version" }

    $tmp = Join-Path ([IO.Path]::GetTempPath()) ("lotar-install-" + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        $asset = "lotar-$Version-$platform"
        $zip = Join-Path $tmp "$asset.zip"
        Write-Host "Downloading LoTaR $Version ($platform)"
        try {
            Invoke-WebRequest -Uri "$Releases/download/$Version/$asset.zip" -OutFile $zip -UseBasicParsing
        } catch {
            # Releases before the native ARM64 build (DEV-309) only have x64,
            # which Windows 11 runs under emulation.
            if ($platform -ne 'windows-arm64') { throw }
            $platform = 'windows-x64'
            $asset = "lotar-$Version-$platform"
            $zip = Join-Path $tmp "$asset.zip"
            Write-Host "$Version has no ARM64 build; installing the x64 build, which Windows 11 runs under emulation."
            Invoke-WebRequest -Uri "$Releases/download/$Version/$asset.zip" -OutFile $zip -UseBasicParsing
        }
        $sumFile = Join-Path $tmp "$asset.sha256"
        Invoke-WebRequest -Uri "$Releases/download/$Version/$asset.sha256" -OutFile $sumFile -UseBasicParsing

        $expected = ((Get-Content -Raw $sumFile).Trim() -split '\s+')[0]
        $actual = (Get-FileHash -Path $zip -Algorithm SHA256).Hash
        # -ne compares case-insensitively; the release writes an uppercase hash.
        if (-not $expected -or $expected -ne $actual) {
            throw "Checksum mismatch for $asset.zip (expected $expected, got $actual); nothing was installed."
        }

        Expand-Archive -Path $zip -DestinationPath $tmp -Force
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
        $exe = Join-Path $InstallDir 'lotar.exe'
        try {
            Copy-Item -Path (Join-Path $tmp 'lotar.exe') -Destination $exe -Force
        } catch {
            throw "Could not write $exe. If lotar is running (for example lotar serve), stop it and try again. $_"
        }
    } finally {
        Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
    }

    # Read and write the raw registry value: [Environment]::Get/SetEnvironmentVariable
    # would expand %VAR% entries and store the user Path as plain text, freezing
    # entries such as %JAVA_HOME%\bin (DEV-291).
    $pathChanged = $false
    $envKey = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
    try {
        $userPath = [string]$envKey.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $entries = @($userPath -split ';' | Where-Object { $_ })
        $expanded = @($entries | ForEach-Object { [Environment]::ExpandEnvironmentVariables($_).TrimEnd('\') })
        if ($expanded -notcontains $InstallDir.TrimEnd('\')) {
            $envKey.SetValue('Path', (($entries + $InstallDir) -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
            $pathChanged = $true
            Write-Host "Added $InstallDir to your user PATH; open a new terminal to use it there."
        }
    } finally {
        $envKey.Close()
    }
    if ($pathChanged) {
        # Setting and clearing a variable through .NET broadcasts WM_SETTINGCHANGE,
        # so Explorer and new terminals pick up the new Path.
        [Environment]::SetEnvironmentVariable('LOTAR_INSTALL_REFRESH', '1', 'User')
        [Environment]::SetEnvironmentVariable('LOTAR_INSTALL_REFRESH', $null, 'User')
    }
    if (($env:Path -split ';') -notcontains $InstallDir) { $env:Path = "$env:Path;$InstallDir" }

    $installed = try { (& $exe --version) -join ' ' } catch { 'lotar' }
    Write-Host "Installed $installed to $exe"
    $found = Get-Command lotar -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($found -and $found.Source -ne $exe) {
        Write-Host "Note: $($found.Source) comes first on your PATH and will run instead."
    }
}

Install-Lotar

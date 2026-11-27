# Installs spacetify on Windows, in one line from PowerShell:
#
#   iwr -useb https://raw.githubusercontent.com/spacetify-mod/spacetify/main/install.ps1 | iex
#
# It downloads the installer of the latest release from GitHub, checks it
# against the SHA-256 published with the release, then opens it. Nothing is
# installed if the check fails. No admin rights needed: the installer works
# for the current user only.
#
# To install a given version instead of the latest one, set it first:
#   $env:SPACETIFY_VERSION = "v0.1.0"

& {
    $ErrorActionPreference = "Stop"
    $ProgressPreference = "SilentlyContinue"
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $repo = "spacetify-mod/spacetify"
    $fr = (Get-Culture).TwoLetterISOLanguageName -eq "fr"

    # The French accents are written as \u00e9 and so on: `iwr | iex` reads the
    # script in the wrong encoding in Windows PowerShell, so it stays ASCII.
    function Fr([string]$text) { [regex]::Unescape($text) }

    function Say([string]$en, [string]$french) {
        if ($fr) { Write-Host (Fr $french) } else { Write-Host $en }
    }

    function Fail([string]$en, [string]$french) {
        Write-Host ""
        if ($fr) { Write-Host (Fr $french) -ForegroundColor Red } else { Write-Host $en -ForegroundColor Red }
        Write-Host "https://github.com/$repo/releases"
    }

    if (-not [Environment]::Is64BitOperatingSystem) {
        Fail "spacetify needs a 64-bit Windows." "spacetify demande un Windows 64 bits."
        return
    }

    $version = $env:SPACETIFY_VERSION
    if ($version) {
        $api = "https://api.github.com/repos/$repo/releases/tags/$version"
    } else {
        $api = "https://api.github.com/repos/$repo/releases/latest"
    }

    Say "Looking for the latest version of spacetify..." "Recherche de la derni\u00e8re version de spacetify..."
    try {
        $release = Invoke-RestMethod -Uri $api -UseBasicParsing -Headers @{ "User-Agent" = "spacetify-install" }
    } catch {
        $code = $null
        if ($_.Exception.Response) { $code = [int]$_.Exception.Response.StatusCode }
        if ($code -eq 404) {
            if ($version) {
                Fail "There is no version $version of spacetify." "Il n'existe pas de version $version de spacetify."
            } else {
                Fail "No version of spacetify has been released yet." "Aucune version de spacetify n'a encore \u00e9t\u00e9 publi\u00e9e."
            }
        } else {
            Fail "Could not reach GitHub. Check your connection and try again." "Impossible de joindre GitHub. V\u00e9rifie ta connexion et r\u00e9essaie."
        }
        return
    }

    $setup = $release.assets | Where-Object { $_.name -like "*_x64-setup.exe" } | Select-Object -First 1
    if (-not $setup) {
        Fail "Version $($release.tag_name) has no Windows installer." "La version $($release.tag_name) n'a pas d'installeur Windows."
        return
    }

    # The SHA-256 of every file is published with the release.
    $expected = $null
    foreach ($sums in ($release.assets | Where-Object { $_.name -like "SHA256SUMS*.txt" })) {
        $text = (Invoke-WebRequest -Uri $sums.browser_download_url -UseBasicParsing -Headers @{ "User-Agent" = "spacetify-install" }).Content
        if ($text -is [byte[]]) { $text = [Text.Encoding]::ASCII.GetString($text) }
        foreach ($line in ($text -split "`r?`n")) {
            $parts = $line.Trim() -split "\s+", 2
            if ($parts.Count -eq 2 -and $parts[1].TrimStart("*") -eq $setup.name) {
                $expected = $parts[0].ToLower()
            }
        }
        if ($expected) { break }
    }
    if (-not $expected) {
        Fail "The SHA-256 of the installer is not published, so it cannot be checked. Nothing was installed." "L'empreinte SHA-256 de l'installeur n'est pas publi\u00e9e, il ne peut donc pas \u00eatre v\u00e9rifi\u00e9. Rien n'a \u00e9t\u00e9 install\u00e9."
        return
    }

    $dir = Join-Path $env:TEMP "spacetify-install"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $file = Join-Path $dir $setup.name

    $size = [math]::Round($setup.size / 1MB, 1)
    Say "Downloading spacetify $($release.tag_name) ($size MB)..." "T\u00e9l\u00e9chargement de spacetify $($release.tag_name) ($size Mo)..."
    try {
        Invoke-WebRequest -Uri $setup.browser_download_url -OutFile $file -UseBasicParsing -Headers @{ "User-Agent" = "spacetify-install" }
    } catch {
        Fail "The download failed. Check your connection and try again." "Le t\u00e9l\u00e9chargement a \u00e9chou\u00e9. V\u00e9rifie ta connexion et r\u00e9essaie."
        return
    }

    $actual = (Get-FileHash -Path $file -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $expected) {
        Remove-Item -Force $file
        Fail "The downloaded file does not match its SHA-256. It was deleted and nothing was installed." "Le fichier t\u00e9l\u00e9charg\u00e9 ne correspond pas \u00e0 son empreinte SHA-256. Il a \u00e9t\u00e9 supprim\u00e9 et rien n'a \u00e9t\u00e9 install\u00e9."
        return
    }
    Say "File checked (SHA-256)." "Fichier v\u00e9rifi\u00e9 (SHA-256)."

    Say "Opening the installer: follow its window." "Ouverture de l'installeur : suis sa fen\u00eatre."
    Start-Process -FilePath $file -Wait
    Remove-Item -Force $file -ErrorAction SilentlyContinue

    Write-Host ""
    Say "Done. Open a new terminal to use the spacetify command." "C'est fini. Ouvre un nouveau terminal pour utiliser la commande spacetify."
}

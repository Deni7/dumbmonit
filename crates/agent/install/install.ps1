<#
.SYNOPSIS
    Installs the DumbMonit system agent and its Windows service.

.DESCRIPTION
    Downloads the binary, writes the configuration with the token, registers the
    service and starts it. Running the script again updates the agent: it is also
    the upgrade procedure. A machine still running the agent under its former
    name (EzyMonitAgent) is migrated in place by the same command.

.EXAMPLE
    & ([scriptblock]::Create((irm https://serveur:8080/install.ps1))) -Token dmon_xxx -Url https://serveur:8080 -Sha256 windows-x86_64:<hex>

    The install command shown by the server carries -Sha256: a downloaded binary
    that does not match it, or a download without it, is refused. With a
    self-signed server certificate, it also carries -PinnedKey and checks the
    server's key before downloading anything.

.EXAMPLE
    .\install.ps1 -Token dmon_xxx -Url https://serveur:8080 -PinnedKey sha256//<base64> -Services @('Spooler','MSSQLSERVER')
#>
[CmdletBinding()]
param(
    [string]$Token,
    # Server URL, https:// only: DumbMonit never serves plain HTTP.
    [string]$Url,
    # Public key of the server's certificate (sha256//<base64>, as curl --pinnedpubkey takes it),
    # when it is self-signed: downloads are then accepted from that key only, and the agent keeps it.
    [string]$PinnedKey,
    # Encrypted agent channel, for example https://server:8443 (default: the host of -Url, port 8443).
    [string]$TlsUrl,
    # Fingerprint (SHA-256) of the server's certificate authority: the agent then only talks to
    # that server, over TLS 1.3 with its own client certificate, never in clear.
    [string]$CaPin,
    [int]$Interval = 0,
    [string[]]$Services = @(),
    [hashtable]$Tags = @{},
    [string]$HostName,
    # Relay mode: the agent also probes the devices of its network for the server.
    [switch]$Relay,
    # Site of this relay, for example "Lyon office".
    [string]$Site,
    # This machine is a relay: open its gateway port on the Windows firewall, inbound, to the
    # local subnet only. Implied by -Relay. Undone by -Uninstall.
    [switch]$OpenGateway,
    # Port of the relay gateway.
    [int]$GatewayPort = 8443,
    # Do not touch the Windows firewall.
    [switch]$NoFirewall,
    # Outbound HTTP proxy to reach the server: http://[user:password@]host:port, 'system' (the
    # WinHTTP proxy of the machine, see 'netsh winhttp show proxy') or 'none'. Used for the
    # downloads here and written to agent.yaml; the agent's TLS goes through it end to end.
    [string]$Proxy,
    # Hosts reached without the proxy, comma-separated.
    [string]$NoProxy,
    # Optional modules to run (docker, plakar, system_health, sensors, smart, zfs,
    # libvirt, wireguard, ipsec, perf_counters, or none). Omitted: every module detects itself.
    [string[]]$Modules,
    # Local binary to install instead of downloading it.
    [string]$BinPath,
    # Expected SHA-256 of the downloaded binary: windows-x86_64:<hex>, or the bare hex.
    [string]$Sha256,
    # Install a downloaded binary without verifying it (air-gapped or manual setups only).
    [switch]$InsecureSkipChecksum,
    # Install only if the agent binary is exactly this version (x.y.z): the server ships its own,
    # and this refuses anything else, for example after a server upgrade.
    [string]$Version,
    # Say what would be done, then stop: nothing is downloaded, written or started.
    [switch]$DryRun,
    [switch]$Uninstall
)

$ErrorActionPreference = 'Stop'

# Les noms de modules sont vérifiés avant toute chose : une faute de frappe ferait
# refuser le démarrage de l'agent, une fois tout installé.
$ModulesConnus = @('docker', 'plakar', 'system_health', 'sensors', 'smart', 'zfs', 'libvirt', 'wireguard', 'ipsec', 'perf_counters', 'none')
foreach ($module in @($Modules | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ })) {
    if ($ModulesConnus -notcontains $module) {
        throw "Unknown module '$module' (known: $($ModulesConnus -join ', '))."
    }
}

$ServiceName = 'DumbMonitAgent'
$InstallDir  = Join-Path $env:ProgramFiles 'DumbMonit'
$ConfigDir   = Join-Path $env:ProgramData 'DumbMonit'
$ExePath     = Join-Path $InstallDir 'dumbmonit-agent.exe'
$ConfigPath  = Join-Path $ConfigDir 'agent.yaml'

# Noms d'avant le renommage EzyMonit → DumbMonit. Une installation qui les porte
# encore est migrée sur place à l'installation, et -Uninstall en fait aussi le
# ménage.
$LegacyServiceName = 'EzyMonitAgent'
$LegacyInstallDir  = Join-Path $env:ProgramFiles 'EzyMonit'
$LegacyConfigDir   = Join-Path $env:ProgramData 'EzyMonit'

function Write-Etape($message) { Write-Host "==> $message" }

# Règle du pare-feu Windows pour la passerelle du relais. Le même nom sert de
# Name et de DisplayName : `netsh advfirewall … name=` vise le second, et c'est
# lui que la désinstallation (ce script, ou l'agent sur ordre du serveur) retire.
$RegleParefeu = 'DumbMonit-relay-gateway'

function Remove-RegleParefeu {
    & netsh.exe advfirewall firewall delete rule name=$RegleParefeu | Out-Null
}

# Ouvre le port de la passerelle du relais, en entrée, au seul sous-réseau local
# (`LocalSubnet`), et le dit. Une règle précédente du même nom est remplacée.
function Open-ParefeuPasserelle {
    if (-not ($OpenGateway -or $Relay)) { return }
    Remove-RegleParefeu
    if ($NoFirewall) {
        Write-Etape "-NoFirewall: open TCP $GatewayPort on this machine's firewall yourself, to the site's network only"
        return
    }
    try {
        if (Get-Command New-NetFirewallRule -ErrorAction SilentlyContinue) {
            New-NetFirewallRule -Name $RegleParefeu -DisplayName $RegleParefeu `
                -Description 'DumbMonit relay gateway: agents of this site connect here.' `
                -Direction Inbound -Protocol TCP -LocalPort $GatewayPort `
                -RemoteAddress LocalSubnet -Action Allow -Profile Any | Out-Null
        } else {
            & netsh.exe advfirewall firewall add rule name=$RegleParefeu dir=in action=allow `
                protocol=TCP localport=$GatewayPort remoteip=localsubnet | Out-Null
            if ($LASTEXITCODE -ne 0) { throw "netsh exited with code $LASTEXITCODE" }
        }
        Write-Etape "Windows firewall: TCP $GatewayPort open to the local subnet only, for the relay gateway (rule $RegleParefeu)"
    } catch {
        Write-Warning "The Windows firewall rule could not be added ($_). Open TCP $GatewayPort inbound yourself, to the site's network only."
    }
}

function Stop-Sur($message) {
    Restore-ValidationTls
    Write-Error $message
    exit 1
}

# Le serveur ne parle que HTTPS : TLS 1.2 au moins (Windows PowerShell 5 ne le
# propose pas toujours d'office), et, pour un certificat autosigné, seul celui
# dont la clé publique a l'empreinte -PinnedKey est accepté — l'en-tête
# SubjectPublicKeyInfo d'une clé P-256 suivi de son point, comme le calcule
# curl. Le rappel vaut pour toute la session : il est retiré en sortie.
function Use-ServerTls {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    if (-not $PinnedKey) { return }
    [Net.ServicePointManager]::ServerCertificateValidationCallback = [scriptblock]::Create(
        "param(`$s, `$c) ('sha256//' + [Convert]::ToBase64String([Security.Cryptography.SHA256]::Create().ComputeHash([byte[]]((48,89,48,19,6,7,42,134,72,206,61,2,1,6,8,42,134,72,206,61,3,1,7,3,66,0) + `$c.GetPublicKey())))) -ceq '$PinnedKey'")
}

function Restore-ValidationTls {
    if ($PinnedKey) { [Net.ServicePointManager]::ServerCertificateValidationCallback = $null }
}

# Mandataire des téléchargements de ce script : celui donné par -Proxy (avec ses
# identifiants), aucun pour 'none', celui du système sinon (le défaut de
# WebClient). Seul http:// est accepté, comme l'agent.
function Use-Mandataire {
    if (-not $Proxy -or $Proxy -eq 'system') { return }
    if ($Proxy -eq 'none') {
        [Net.WebRequest]::DefaultWebProxy = $null
        return
    }
    if ($Proxy -notmatch '^http://') {
        Stop-Sur "-Proxy expects http://host:port, system or none (the agent's own TLS goes through it)."
    }
    $adresse = [Uri]$Proxy
    $mandataire = New-Object Net.WebProxy("http://$($adresse.Authority)", $true)
    if ($NoProxy) {
        $mandataire.BypassList = @($NoProxy -split ',' | ForEach-Object { [Regex]::Escape($_.Trim()) } | Where-Object { $_ })
    }
    if ($adresse.UserInfo) {
        $nom, $secret = $adresse.UserInfo -split ':', 2
        $mandataire.Credentials = New-Object Net.NetworkCredential(
            [Uri]::UnescapeDataString($nom), [Uri]::UnescapeDataString([string]$secret))
    }
    [Net.WebRequest]::DefaultWebProxy = $mandataire
    Write-Etape "Using the proxy $($adresse.Authority) to reach the server"
}

# Le canal chiffré des agents répond-il ? Une simple connexion TCP, bornée à
# cinq secondes : « refused » (port non publié, rien n'écoute), « timeout »
# (pare-feu qui jette les paquets), « ok » sinon.
function Test-CanalAgents {
    $canal = $TlsUrl
    if (-not $canal) { $canal = "https://$(([Uri]$Url).Host):8443" }
    $cible = [Uri]$canal
    $client = New-Object System.Net.Sockets.TcpClient
    try {
        $tentative = $client.BeginConnect($cible.Host, $cible.Port, $null, $null)
        if (-not $tentative.AsyncWaitHandle.WaitOne(5000)) { return @('timeout', $cible) }
        $client.EndConnect($tentative)
        return @('ok', $cible)
    } catch {
        return @('refused', $cible)
    } finally {
        $client.Close()
    }
}

# Arrête et retire le service et le dossier d'installation de l'ancien agent.
# Ne touche pas à sa configuration. Silencieux quand rien d'ancien n'est
# présent ; renvoie $true si quelque chose a été retiré.
function Remove-AncienAgent {
    $retire = $false
    if (Get-Service -Name $LegacyServiceName -ErrorAction SilentlyContinue) {
        Stop-Service -Name $LegacyServiceName -Force -ErrorAction SilentlyContinue
        & sc.exe delete $LegacyServiceName | Out-Null
        $retire = $true
    }
    if (Test-Path $LegacyInstallDir) {
        Remove-Item -Path $LegacyInstallDir -Recurse -Force -ErrorAction SilentlyContinue
        $retire = $true
    }
    return $retire
}

# Propriétaires admis pour le dossier de configuration et ce qu'il contient :
# SYSTEM, les administrateurs, TrustedInstaller, et le compte qui exécute
# l'installateur (déjà administrateur, contrôlé plus bas).
$SidSysteme = New-Object Security.Principal.SecurityIdentifier('S-1-5-18')
$SidAdmins  = New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544')

function Test-ProprietaireSur($chemin) {
    $proprietaire = (Get-Acl -LiteralPath $chemin).GetOwner([Security.Principal.SecurityIdentifier])
    $admis = @(
        'S-1-5-18',
        'S-1-5-32-544',
        'S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464',
        [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    )
    return $admis -contains $proprietaire.Value
}

# Le dossier de configuration porte le jeton, le secret de liaison, et une
# configuration que le service SYSTEM exécute (password_command des dépôts
# restic). Sous ProgramData, il hériterait de droits qui laissent tout
# utilisateur y créer des fichiers : un compte sans privilège qui l'aurait créé
# avant nous en resterait propriétaire, et pourrait réécrire la configuration.
# Un dossier ou un fichier appartenant à un autre compte arrête donc
# l'installation ; sinon, le dossier est réservé à SYSTEM et aux
# administrateurs (héritage coupé, propriétaire Administrators), puis tout son
# contenu est réaligné sur ces seuls droits.
function Protect-ConfigDir {
    if (Test-Path -LiteralPath $ConfigDir) {
        $elements = @(Get-Item -LiteralPath $ConfigDir -Force) + @(Get-ChildItem -LiteralPath $ConfigDir -Recurse -Force)
        foreach ($element in $elements) {
            if (-not (Test-ProprietaireSur $element.FullName)) {
                Stop-Sur "$($element.FullName) belongs to another account: it may have been planted to take over the agent. Check it, remove $ConfigDir, then run the installer again."
            }
        }
    } else {
        New-Item -ItemType Directory -Path $ConfigDir -Force | Out-Null
    }
    & icacls.exe $ConfigDir /setowner '*S-1-5-32-544' /T /C /Q | Out-Null
    $acl = New-Object System.Security.AccessControl.DirectorySecurity
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($sid in @($SidSysteme, $SidAdmins)) {
        $regle = New-Object System.Security.AccessControl.FileSystemAccessRule(
            $sid, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
        $acl.AddAccessRule($regle)
    }
    Set-Acl -LiteralPath $ConfigDir -AclObject $acl
    # Le contenu ne garde que ce qu'il hérite du dossier.
    Get-ChildItem -LiteralPath $ConfigDir -Recurse -Force | ForEach-Object {
        & icacls.exe $_.FullName /reset /C /Q | Out-Null
    }
}

# Migration sur place d'une installation d'avant le renommage : l'ancien service
# est arrêté et retiré, et sa configuration reprend sa place sous le nouveau nom.
# Elle est réécrite juste après depuis -Token/-Url, mais les clés qu'un
# utilisateur y aurait ajoutées restent à portée de main. Sans rien d'ancien,
# ne fait rien et ne dit rien.
function Migrate-Legacy {
    $migre = Remove-AncienAgent
    if ((Test-Path $LegacyConfigDir) -and -not (Test-Path $ConfigDir)) {
        # Sur un même volume, Move-Item est un renommage : les droits posés par
        # icacls sur l'ancien fichier de configuration sont conservés.
        Move-Item -Path $LegacyConfigDir -Destination $ConfigDir
        $migre = $true
    }
    if ($migre) { Write-Etape "Migrating the $LegacyServiceName install to $ServiceName" }
}

# --------------------------------------------------------------- privilèges

$identite = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identite)
if ($Version -and $Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$') {
    Stop-Sur "Invalid -Version value (expected x.y.z): $Version"
}
if (-not $DryRun -and -not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Stop-Sur "This installer needs a PowerShell console running as administrator."
}

# ------------------------------------------------------------ désinstallation

if ($Uninstall) {
    if ($DryRun) {
        Write-Etape "Dry run: nothing is changed. The uninstall would:"
        Write-Etape "  - stop and delete the service $ServiceName"
        Write-Etape "  - delete $InstallDir and the configuration $ConfigDir (it holds the token)"
        Write-Etape "  - remove the firewall rule of the relay gateway, if any"
        exit 0
    }
    Write-Etape "Stopping the service"
    if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) {
        Stop-Service -Name $ServiceName -Force -ErrorAction SilentlyContinue
        # `sc.exe delete` plutôt que `Remove-Service` : ce dernier n'existe qu'à
        # partir de PowerShell 6, absent de bien des Windows Server encore en place.
        & sc.exe delete $ServiceName | Out-Null
    }
    Remove-Item -Path $InstallDir -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -Path $ConfigDir -Recurse -Force -ErrorAction SilentlyContinue
    # La règle du pare-feu de la passerelle, si ce relais en avait une.
    Remove-RegleParefeu
    # Les restes d'une installation d'avant le renommage partent aussi.
    Remove-AncienAgent | Out-Null
    Remove-Item -Path $LegacyConfigDir -Recurse -Force -ErrorAction SilentlyContinue
    Write-Etape "Agent uninstalled"
    exit 0
}

if ([string]::IsNullOrWhiteSpace($Token)) { Stop-Sur "The enrollment token is required (-Token)." }
if ([string]::IsNullOrWhiteSpace($Url))   { Stop-Sur "The server URL is required (-Url https://server:8080)." }
$Url = $Url.TrimEnd('/')
if ($Url -notmatch '^https://') {
    Stop-Sur "The server URL must start with https:// (got $Url): DumbMonit only serves HTTPS. Copy a fresh install command from DumbMonit."
}
if ($PinnedKey -and $PinnedKey -cnotmatch '^sha256//[A-Za-z0-9+/]{43}=$') {
    Stop-Sur "Invalid -PinnedKey value (expected sha256//<base64>): $PinnedKey"
}
if ($GatewayPort -lt 1 -or $GatewayPort -gt 65535) {
    Stop-Sur "Invalid -GatewayPort value: $GatewayPort"
}
Use-Mandataire
Use-ServerTls

# ------------------------------------------------------------------ binaire

# Un seul binaire Windows est construit, pour x86_64. Windows 11 sur ARM64 le fait
# tourner en émulation x64 ; Windows 10 sur ARM64 ne sait émuler que du 32 bits.
$architecture = switch ($env:PROCESSOR_ARCHITECTURE) {
    'AMD64' { 'x86_64' }
    'ARM64' {
        Write-Warning "No native ARM64 build of the agent: installing the x86_64 one, which Windows 11 runs under x64 emulation."
        'x86_64'
    }
    default { Stop-Sur "Unsupported architecture: $env:PROCESSOR_ARCHITECTURE" }
}

if ($DryRun) {
    Write-Etape "Dry run: nothing is downloaded, written or started. The install would:"
    Write-Etape "  - system: windows-$architecture, service: $ServiceName"
    if ($BinPath) {
        Write-Etape "  - install the binary $BinPath to $ExePath"
    } else {
        Write-Etape "  - download $Url/download/dumbmonit-agent-windows-$architecture.exe and install it to $ExePath"
        if ($InsecureSkipChecksum) {
            Write-Etape "  - NOT verify its checksum (-InsecureSkipChecksum)"
        } elseif ($Sha256) {
            Write-Etape "  - verify its SHA-256 against -Sha256"
        } else {
            Write-Etape "  - REFUSE it: no expected SHA-256 (-Sha256 missing)"
        }
    }
    if ($Version) { Write-Etape "  - refuse any binary that is not version $Version" }
    Write-Etape "  - write the configuration in $ConfigDir (server $Url, token hidden)"
    Write-Etape "  - register and start the service $ServiceName"
    exit 0
}

New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
# Le dossier de configuration n'est créé qu'une fois protégé (Protect-ConfigDir),
# juste avant d'y écrire : pas de fenêtre où il hériterait des droits de
# ProgramData.

# Écriture à côté puis renommage : Windows verrouille le fichier d'un exécutable
# en cours, et un service déjà installé tiendrait le sien.
$exeTemporaire = "$ExePath.nouveau"

if ($BinPath) {
    if (-not (Test-Path $BinPath)) { Stop-Sur "Binary not found: $BinPath" }
    Write-Etape "Installing from $BinPath"
    Copy-Item -Path $BinPath -Destination $exeTemporaire -Force
} else {
    $source = "$Url/download/dumbmonit-agent-windows-$architecture.exe"
    Write-Etape "Downloading $source"
    try {
        # `WebClient` plutôt qu'`Invoke-WebRequest` : il suit le rappel de
        # validation de la session (l'épingle) sous Windows PowerShell 5 comme
        # sous PowerShell 7, et n'a pas de barre de progression qui ralentit.
        (New-Object Net.WebClient).DownloadFile($source, $exeTemporaire)
    } catch {
        Stop-Sur "Download failed from ${source} (is the server certificate the expected one?): $_"
    }

    # Le binaire n'est installé que s'il correspond à l'empreinte portée par la
    # commande d'installation (-Sha256), copiée depuis l'interface : un binaire
    # remplacé en chemin ou tronqué est refusé, et une empreinte absente aussi,
    # sauf -InsecureSkipChecksum, qui le dit haut et fort.
    if ($InsecureSkipChecksum) {
        Write-Warning "-InsecureSkipChecksum: the downloaded binary is NOT verified."
        Write-Warning "Anyone able to alter the download now runs code as SYSTEM on this machine."
    } else {
        $cle = "windows-$architecture"
        $attendu = $null
        foreach ($entree in ($Sha256 -split ',')) {
            $entree = $entree.Trim().ToLowerInvariant()
            if (-not $entree) { continue }
            if ($entree.StartsWith("${cle}:")) { $attendu = $entree.Substring($cle.Length + 1); break }
            if (-not $entree.Contains(':')) { $attendu = $entree; break }
        }
        if (-not $attendu) {
            Remove-Item -Path $exeTemporaire -Force -ErrorAction SilentlyContinue
            Stop-Sur "No expected checksum for ${cle}: nothing was installed. Copy a fresh install command from DumbMonit (Add monitor > Server with the agent): it carries -Sha256. For a binary you verified yourself, use -BinPath."
        }
        if ($attendu -notmatch '^[0-9a-f]{64}$') {
            Remove-Item -Path $exeTemporaire -Force -ErrorAction SilentlyContinue
            Stop-Sur "Invalid -Sha256 value for ${cle}: $attendu"
        }
        $obtenu = (Get-FileHash -Path $exeTemporaire -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($obtenu -ne $attendu) {
            Remove-Item -Path $exeTemporaire -Force -ErrorAction SilentlyContinue
            Stop-Sur "Checksum mismatch for ${source} (expected $attendu, got $obtenu): the download is corrupt or has been tampered with, or the server was upgraded since this command was copied. Nothing was installed. Copy a fresh install command from DumbMonit (Add monitor > Server with the agent)."
        }
        Write-Etape "Checksum verified ($obtenu)"
    }
}

# Version imposée : un autre binaire n'est pas installé, l'agent en place reste tel quel.
if ($Version) {
    $obtenue = ''
    try { $obtenue = ((& $exeTemporaire --version) -join ' ').Trim() -replace '^dumbmonit-agent\s+', '' } catch { }
    if ($obtenue -ne $Version) {
        Remove-Item -Path $exeTemporaire -Force -ErrorAction SilentlyContinue
        Stop-Sur "The agent binary is version '$obtenue', not the requested $Version: nothing was installed."
    }
    Write-Etape "Version $obtenue confirmed"
}

if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) {
    Write-Etape "Stopping the existing service before replacing it"
    Stop-Service -Name $ServiceName -Force -ErrorAction SilentlyContinue
}
Move-Item -Path $exeTemporaire -Destination $ExePath -Force

# Le nouveau binaire est en place : l'ancien agent, s'il est là, peut être
# arrêté et sa configuration déplacée avant que la nouvelle ne soit écrite. Pas
# avant — un téléchargement raté ne doit pas laisser la machine sans agent.
Migrate-Legacy
Protect-ConfigDir

# ------------------------------------------------------------ configuration

$lignes = New-Object System.Collections.Generic.List[string]
$lignes.Add("# DumbMonit system agent configuration.")
$lignes.Add("# Written by install.ps1 — environment variables override it.")
$lignes.Add("server_url: $Url")
$lignes.Add("token: $Token")
if ($PinnedKey) { $lignes.Add("server_pin: $PinnedKey") }
if ($TlsUrl) { $lignes.Add("tls_url: $TlsUrl") }
if ($CaPin)  { $lignes.Add("ca_pin: $CaPin") }
# Entre apostrophes : des identifiants peuvent contenir « : » ou « # ».
if ($Proxy)   { $lignes.Add("proxy: '$($Proxy -replace "'", "''")'") }
if ($NoProxy) { $lignes.Add("no_proxy: '$($NoProxy -replace "'", "''")'") }
if ($Interval -gt 0)  { $lignes.Add("interval_secs: $Interval") }
if ($HostName)        { $lignes.Add("hostname: $HostName") }
if ($Relay)           { $lignes.Add("relay: true") }
if ($Site)            { $lignes.Add("site: '$($Site -replace "'", "''")'") }
if ($Services.Count -gt 0) {
    $lignes.Add("services:")
    foreach ($service in $Services) { $lignes.Add("  - $service") }
}
if ($Tags.Count -gt 0) {
    $lignes.Add("tags:")
    foreach ($cle in $Tags.Keys) { $lignes.Add("  ${cle}: $($Tags[$cle])") }
}
# Modules choisis : seuls ceux-là tournent. Sans -Modules, la clé est absente et
# chacun se détecte seul.
if ($PSBoundParameters.ContainsKey('Modules')) {
    $choisis = @($Modules | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } |
        Where-Object { $_ -and $_ -ne 'none' })
    if ($choisis.Count -eq 0) {
        $lignes.Add("modules: []")
    } else {
        $lignes.Add("modules:")
        foreach ($module in $choisis) { $lignes.Add("  - $module") }
    }
}

# UTF-8 sans marque d'ordre : l'analyseur YAML de l'agent lit de l'UTF-8 brut, et
# une marque d'ordre en tête ferait échouer la première clé.
$encodage = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllLines($ConfigPath, $lignes, $encodage)

# Le fichier porte le jeton : seuls le système et les administrateurs doivent le
# lire. L'héritage est coupé, sinon les droits du dossier parent le rouvriraient.
& icacls.exe $ConfigPath /inheritance:r /grant:r 'SYSTEM:(R)' 'Administrators:(F)' | Out-Null
Write-Etape "Configuration written to $ConfigPath"

# Passerelle du relais : son port, au seul réseau du site.
Open-ParefeuPasserelle

# --------------------------------------------------------------- vérification

# Un envoi de test avant de démarrer le service : un jeton erroné ou un serveur
# injoignable doit se voir maintenant, pas dans le journal des événements.
Write-Etape "Checking the connection to $Url"
& $ExePath --config="$ConfigPath" --once
if ($LASTEXITCODE -ne 0) {
    $etat, $cible = Test-CanalAgents
    $port = $cible.Port
    if ($etat -eq 'refused') {
        Write-Host @"
The encrypted agent channel $($cible.Host):$port refuses the connection: port $port is
not published or not open on the server.
- Docker: add "$($port):8443" under ports: in docker-compose.yml, then run 'docker compose up -d'.
- Firewall of the server host: 'firewall-cmd --add-port=$port/tcp --permanent' then
  'firewall-cmd --reload', 'ufw allow $port/tcp', or on Windows:
  New-NetFirewallRule -DisplayName 'DumbMonit agents' -Direction Inbound -Protocol TCP -LocalPort $port -Action Allow
"@ -ForegroundColor Red
    } elseif ($etat -eq 'timeout') {
        Write-Host @"
No answer from the encrypted agent channel $($cible.Host):$port (timeout): a firewall drops
the traffic, or the address is wrong. Open TCP port $port towards the server, and publish it
in Docker ("$($port):8443").
"@ -ForegroundColor Red
    }
    Stop-Sur @"
The agent could not reach the server — check the URL and the token.
The configuration is in place: fix $ConfigPath, then run
'Start-Service $ServiceName'.
"@
}

# ------------------------------------------------------------------- service

$commande = "`"$ExePath`" --service --config=`"$ConfigPath`""

if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) {
    Write-Etape "Updating the existing service"
    # La ligne de commande est écrite directement dans le registre plutôt que
    # passée à `sc.exe config` : Windows PowerShell 5 transmet mal à un programme
    # externe un argument qui contient à la fois des espaces et des guillemets,
    # et `sc.exe` recevrait un chemin coupé à « C:\Program ».
    Set-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Services\$ServiceName" `
        -Name ImagePath -Value $commande
    Set-Service -Name $ServiceName -StartupType Automatic
} else {
    Write-Etape "Registering the service"
    New-Service -Name $ServiceName `
        -BinaryPathName $commande `
        -DisplayName "DumbMonit system agent" `
        -Description "Collects this machine's metrics and pushes them to the DumbMonit server." `
        -StartupType Automatic | Out-Null
}

# Redémarrage automatique après une panne : sans cela, un agent tombé une fois ne
# revient qu'au prochain redémarrage de la machine — soit jamais, sur un serveur.
& sc.exe failure $ServiceName reset= 86400 actions= restart/10000/restart/30000/restart/60000 | Out-Null

Start-Service -Name $ServiceName
Restore-ValidationTls

Write-Etape "Agent installed and started"
Write-Etape "Status: Get-Service $ServiceName"
Write-Etape "Logs:   Get-Content '$(Join-Path $ConfigDir 'agent.log')' -Tail 20 -Wait"

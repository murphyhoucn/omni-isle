$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$templateRoot = Join-Path $repoRoot 'dev-configs'

if (!(Test-Path $templateRoot)) {
  throw "Template folder missing: $templateRoot"
}

$userProfile = $env:USERPROFILE
$localAppData = $env:LOCALAPPDATA
if ([string]::IsNullOrWhiteSpace($userProfile) -or [string]::IsNullOrWhiteSpace($localAppData)) {
  throw 'USERPROFILE or LOCALAPPDATA is missing.'
}

$settingsRoot = Join-Path $userProfile '.omniisle'
$dataRoot = Join-Path $localAppData 'OmniIsle'
$scriptsRoot = Join-Path $dataRoot 'scripts'
$logsRoot = Join-Path $dataRoot 'logs'
$tmpRoot = Join-Path $dataRoot 'tmp'

New-Item -ItemType Directory -Path $settingsRoot -Force | Out-Null
New-Item -ItemType Directory -Path $dataRoot -Force | Out-Null
New-Item -ItemType Directory -Path $scriptsRoot -Force | Out-Null
New-Item -ItemType Directory -Path $logsRoot -Force | Out-Null
New-Item -ItemType Directory -Path $tmpRoot -Force | Out-Null

# .omniisle only keeps app_configs.json
$targetAppConfig = Join-Path $settingsRoot 'app_configs.json'
$targetSettingsFiles = Get-ChildItem -Path $settingsRoot -File -ErrorAction SilentlyContinue
foreach ($file in $targetSettingsFiles) {
  if ($file.Name -ne 'app_configs.json') {
    Remove-Item -Path $file.FullName -Force
  }
}

$templateAppConfigPath = Join-Path $templateRoot 'app_jsons.json'
$templateApp = Get-Content -Path $templateAppConfigPath -Raw | ConvertFrom-Json
$templateApp.system_integration.user_data_path = $dataRoot
$templateAppJson = $templateApp | ConvertTo-Json -Depth 8
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($targetAppConfig, $templateAppJson, $utf8NoBom)

Copy-Item -Path (Join-Path $templateRoot 'scripts_configs.json') -Destination (Join-Path $dataRoot 'scripts_config.json') -Force
Copy-Item -Path (Join-Path $templateRoot 'env_configs.json') -Destination (Join-Path $dataRoot 'env_config.json') -Force

$templateScripts = Join-Path $templateRoot 'scripts'
if (Test-Path $templateScripts) {
  Copy-Item -Path (Join-Path $templateScripts '*') -Destination $scriptsRoot -Recurse -Force
}

Write-Output 'DEV_TEMPLATE_SETUP_OK'
Write-Output "Settings root: $settingsRoot"
Write-Output "Data root: $dataRoot"

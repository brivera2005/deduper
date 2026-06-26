$ErrorActionPreference = 'SilentlyContinue'
$results = @()

function Test-IsLocalDriveItem($item) {
  $name = [string]$item.Name
  $type = [string]$item.Type
  if ($name -match '^[A-Z]:$') { return $true }
  if ($type -match 'Local Disk|Fixed Disk|Network Location|Network Drive|CD Drive|DVD Drive|BD-ROM|Floppy|System Folder') { return $true }
  if ($name -match '^(Homegroup|Linux|Network|OneDrive|Desktop|Documents|Downloads|Music|Pictures|Videos)$') { return $true }
  return $false
}

function Add-MtpStorage($deviceName, $storageItem) {
  $storageName = [string]$storageItem.Name
  $storageFolder = $storageItem.GetFolder
  if (-not $storageFolder) { return }
  $parsingName = $storageItem.ExtendedProperty('System.ParsingName')
  if (-not $parsingName) { return }
  # MTP/WPD paths use shell GUIDs; local drives use "C:\" style paths.
  if ($parsingName -notmatch '::' -and $parsingName -match '^[A-Z]:\\?$') { return }
  $freeBytes = $null
  $totalBytes = $null
  try {
    $freeProp = $storageItem.ExtendedProperty('System.FreeSpace')
    $capProp = $storageItem.ExtendedProperty('System.Capacity')
    if ($freeProp) { $freeBytes = [uint64]$freeProp }
    if ($capProp) { $totalBytes = [uint64]$capProp }
  } catch {}
  $results += [PSCustomObject]@{
    name = $deviceName
    storage_name = $storageName
    storage_path = [string]$parsingName
    free_bytes = $freeBytes
    total_bytes = $totalBytes
  }
}

try {
  $shell = New-Object -ComObject Shell.Application
  $computer = $shell.NameSpace(0x11)
  if ($computer) {
    foreach ($deviceItem in @($computer.Items())) {
      if (Test-IsLocalDriveItem $deviceItem) { continue }
      $deviceName = [string]$deviceItem.Name
      if ([string]::IsNullOrWhiteSpace($deviceName)) { continue }
      $deviceFolder = $deviceItem.GetFolder
      if (-not $deviceFolder) { continue }
      $storageItems = @($deviceFolder.Items())
      if ($storageItems.Count -eq 0) { continue }
      foreach ($storageItem in $storageItems) {
        Add-MtpStorage $deviceName $storageItem
      }
    }
  }
} catch {}

# Always emit a JSON array (PowerShell omits [] for single objects otherwise).
if ($results.Count -eq 0) {
  Write-Output '[]'
} else {
  ConvertTo-Json -InputObject @($results) -Compress
}

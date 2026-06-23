$ErrorActionPreference = 'SilentlyContinue'
$mediaFolders = @('DCIM','Pictures','Download','Downloads','Movies','Camera','WhatsApp','Screenshots')
$results = @()
try {
  $shell = New-Object -ComObject Shell.Application
  $computer = $shell.NameSpace(0x11)
  if (-not $computer) { Write-Output '[]'; exit 0 }
  foreach ($deviceItem in @($computer.Items())) {
    $deviceType = [string]$deviceItem.Type
    if ($deviceType -notmatch 'Portable|Phone|Device|Android|Mobile') { continue }
    $deviceName = [string]$deviceItem.Name
    $deviceFolder = $deviceItem.GetFolder
    if (-not $deviceFolder) { continue }
    foreach ($storageItem in @($deviceFolder.Items())) {
      $storageName = [string]$storageItem.Name
      $storageFolder = $storageItem.GetFolder
      if (-not $storageFolder) { continue }
      $parsingName = $storageItem.ExtendedProperty('System.ParsingName')
      if (-not $parsingName) { continue }
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
  }
} catch {}
$results | ConvertTo-Json -Compress

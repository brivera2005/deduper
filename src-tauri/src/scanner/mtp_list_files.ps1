$ErrorActionPreference = 'SilentlyContinue'
$mediaFolders = @('DCIM','Pictures','Download','Downloads','Movies','Camera','WhatsApp','Screenshots','Snapchat','Instagram')
$results = @()

function Add-FilesFromFolder($folder, $depth) {
  if ($depth -gt 8) { return }
  foreach ($item in @($folder.Items())) {
    if ($item.IsFolder) {
      Add-FilesFromFolder $item.GetFolder ($depth + 1)
    } else {
      $name = [string]$item.Name
      $path = [string]$item.ExtendedProperty('System.ParsingName')
      if (-not $path) { continue }
      $size = 0
      try { $size = [uint64]$item.Size } catch {}
      $results += [PSCustomObject]@{
        name = $name
        path = $path
        size_bytes = $size
      }
    }
  }
}

try {
  $shell = New-Object -ComObject Shell.Application
  $storageFolder = $shell.NameSpace($storagePath)
  if (-not $storageFolder) {
    Write-Error "Cannot open phone storage. Set USB to File Transfer / MTP."
    exit 1
  }
  foreach ($item in @($storageFolder.Items())) {
    $folderName = [string]$item.Name
    if ($mediaFolders -contains $folderName) {
      $sub = $item.GetFolder
      if ($sub) { Add-FilesFromFolder $sub 0 }
    }
  }
  if ($results.Count -eq 0) {
    Add-FilesFromFolder $storageFolder 0
  }
} catch {
  Write-Error $_.Exception.Message
  exit 1
}
$results | ConvertTo-Json -Compress

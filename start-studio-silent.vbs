Set WshShell = CreateObject("WScript.Shell")
Set FSO = CreateObject("Scripting.FileSystemObject")
WshShell.CurrentDirectory = "E:\shardvault-rs"

Dim dataDir
If WScript.Arguments.Count > 0 Then
    dataDir = WScript.Arguments(0)
ElseIf FSO.FolderExists("E:\DocuTrack\data") Then
    dataDir = "E:\DocuTrack\data"
Else
    dataDir = ".\data"
End If

If FSO.FileExists("E:\shardvault-rs\target\debug\shardvault-studio.exe") Then
    WshShell.Run """E:\shardvault-rs\target\debug\shardvault-studio.exe"" """ & dataDir & """", 1, False
Else
    WshShell.Run "cmd /c cargo run --bin shardvault-studio -- """ & dataDir & """", 1, False
End If

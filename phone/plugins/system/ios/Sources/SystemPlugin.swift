import Foundation
import Tauri

struct FolderArgs: Decodable {
  var path: String
}

/// The iPhone side of the system plugin. Only keeping a folder out of the backup lives here:
/// every other command answers its stand-in on an iPhone.
class SystemPlugin: Plugin {
  /// Marks a folder of the app's own so iCloud and computer backups leave it out. The mark is
  /// on the folder: what is written in it later stays out too.
  @objc public func excludeFromBackup(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(FolderArgs.self)
    var url = URL(fileURLWithPath: args.path, isDirectory: true)
    var values = URLResourceValues()
    values.isExcludedFromBackup = true
    do {
      try url.setResourceValues(values)
      invoke.resolve()
    } catch {
      invoke.reject(error.localizedDescription)
    }
  }
}

@_cdecl("init_plugin_nocturne_system")
func initPlugin() -> Plugin {
  return SystemPlugin()
}

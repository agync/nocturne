import Foundation
import Security

/// A secret the app keeps, such as the key to the person's online service, in the iPhone's
/// keychain: readable after the first unlock so background work can use it, and kept to this
/// device, so no backup or new phone takes it.
enum Safe {
  private static let service = "app.nocturne.safe"

  static func validName(_ name: String) -> Bool {
    (1...32).contains(name.count) && name.allSatisfy { ($0 >= "a" && $0 <= "z") || $0 == "-" }
  }

  private static func query(_ name: String) -> [String: Any] {
    [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: name,
    ]
  }

  static func keep(_ name: String, _ value: String) -> Bool {
    SecItemDelete(query(name) as CFDictionary)
    var item = query(name)
    item[kSecValueData as String] = Data(value.utf8)
    item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
    return SecItemAdd(item as CFDictionary, nil) == errSecSuccess
  }

  static func read(_ name: String) -> String? {
    var item = query(name)
    item[kSecReturnData as String] = true
    item[kSecMatchLimit as String] = kSecMatchLimitOne
    var found: CFTypeRef?
    guard SecItemCopyMatching(item as CFDictionary, &found) == errSecSuccess,
      let data = found as? Data
    else { return nil }
    return String(data: data, encoding: .utf8)
  }

  static func forget(_ name: String) -> Bool {
    let status = SecItemDelete(query(name) as CFDictionary)
    return status == errSecSuccess || status == errSecItemNotFound
  }
}

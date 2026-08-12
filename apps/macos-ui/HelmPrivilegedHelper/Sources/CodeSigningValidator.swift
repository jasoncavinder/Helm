import Darwin
import Foundation
import Security

enum CodeSigningValidator {
    static func process(_ pid: pid_t, satisfiesIdentifier identifier: String) -> Bool {
        guard pid > 0 else { return false }

        var code: SecCode?
        let attributes = [kSecGuestAttributePid: pid] as NSDictionary as CFDictionary
        guard SecCodeCopyGuestWithAttributes(nil, attributes, SecCSFlags(), &code) == errSecSuccess,
              let code else {
            return false
        }

        let requirementText = "anchor apple generic and certificate leaf[subject.OU] = \"\(PrivilegedHelperConstants.teamIdentifier)\" and identifier \"\(identifier)\""
        var requirement: SecRequirement?
        guard SecRequirementCreateWithString(
            requirementText as CFString,
            SecCSFlags(),
            &requirement
        ) == errSecSuccess,
        let requirement else {
            return false
        }

        return SecCodeCheckValidity(code, SecCSFlags(), requirement) == errSecSuccess
    }

    static func parentProcessIdentifier(of pid: pid_t) -> pid_t? {
        guard pid > 0 else { return nil }
        var info = proc_bsdinfo()
        let expectedSize = Int32(MemoryLayout<proc_bsdinfo>.size)
        let actualSize = proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, expectedSize)
        guard actualSize == expectedSize, info.pbi_ppid > 0 else { return nil }
        return pid_t(info.pbi_ppid)
    }
}

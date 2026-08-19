import Foundation
import XCTest

final class LibraryResultProvenanceTests: XCTestCase {
    func testDecodesAndValidatesManagerSnapshotProvenance() throws {
        let provenance = try decode(
            """
            {
              "schema_version": 1,
              "origin": "local",
              "discovery_source": "manager_snapshot",
              "source_manager": "rustup"
            }
            """
        )

        let validated = try XCTUnwrap(provenance.validated(for: "rustup"))
        XCTAssertEqual(validated.origin, .local)
        XCTAssertEqual(validated.discoverySource, .managerSnapshot)
        XCTAssertNil(validated.originatingQuery)
    }

    func testDecodesAndValidatesVersionedCacheProvenance() throws {
        let provenance = try decode(
            """
            {
              "schema_version": 1,
              "origin": "local_cache",
              "discovery_source": "remote_search",
              "source_manager": "cargo",
              "originating_query": "ripgrep",
              "observed_at_unix": 1800000000
            }
            """
        )

        let validated = try XCTUnwrap(provenance.validated(for: "cargo"))
        XCTAssertEqual(validated.origin, .localCache)
        XCTAssertEqual(validated.discoverySource, .remoteSearch)
        XCTAssertEqual(validated.originatingQuery, "ripgrep")
        XCTAssertEqual(validated.observedAtUnix, 1_800_000_000)
    }

    func testRejectsSourceManagerMismatch() throws {
        let provenance = try decode(
            """
            {
              "schema_version": 1,
              "origin": "local_cache",
              "discovery_source": "catalog_sync",
              "source_manager": "homebrew_formula"
            }
            """
        )

        XCTAssertNil(provenance.validated(for: "cargo"))
    }

    func testRejectsNoncanonicalSourceManagerIdentity() throws {
        let provenance = try decode(
            """
            {
              "schema_version": 1,
              "origin": "local_cache",
              "discovery_source": "remote_search",
              "source_manager": "Cargo",
              "originating_query": "ripgrep"
            }
            """
        )

        XCTAssertNil(provenance.validated(for: "cargo"))
    }

    func testRejectsUnknownSchemaAndEnumValuesWithoutFailingDecode() throws {
        let provenance = try decode(
            """
            {
              "schema_version": 2,
              "origin": "future_origin",
              "discovery_source": "future_discovery",
              "source_manager": "cargo"
            }
            """
        )

        XCTAssertEqual(provenance.origin, .unknown)
        XCTAssertEqual(provenance.discoverySource, .unknown)
        XCTAssertNil(provenance.validated(for: "cargo"))
    }

    func testRejectsRemoteDiscoveryWithoutOriginatingQuery() throws {
        let provenance = try decode(
            """
            {
              "schema_version": 1,
              "origin": "local_cache",
              "discovery_source": "remote_search",
              "source_manager": "cargo"
            }
            """
        )

        XCTAssertNil(provenance.validated(for: "cargo"))
    }

    private func decode(_ json: String) throws -> PackageResultProvenance {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(PackageResultProvenance.self, from: Data(json.utf8))
    }
}

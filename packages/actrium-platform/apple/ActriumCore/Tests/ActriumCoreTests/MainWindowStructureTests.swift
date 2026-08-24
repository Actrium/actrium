import XCTest
@testable import ActroCore

final class MainWindowStructureTests: XCTestCase {
    func testCanonicalEntityNamesAndOrder() {
        let structure = MainWindowStructure.canonical

        XCTAssertEqual(structure.root, .mainWindow)
        XCTAssertEqual(structure.topLevelElements, [.tabs, .workspace])
        XCTAssertEqual(MainWindowEntityName.mainWindow.rawValue, "MainWindow")
        XCTAssertEqual(MainWindowEntityName.tabs.rawValue, "Tabs")
        XCTAssertEqual(MainWindowEntityName.workspace.rawValue, "Workspace")
    }

    func testEveryAppleHostAndPresentationUsesCanonicalStructure() {
        for platform in MainWindowHostPlatform.allCases {
            for presentation in MainWindowPresentation.allCases {
                XCTAssertEqual(
                    MainWindowStructure.resolve(
                        platform: platform,
                        presentation: presentation
                    ),
                    .canonical,
                    "\(platform.rawValue)/\(presentation.rawValue) changed the top-level hierarchy"
                )
            }
        }
    }

    func testAccessibilityIdentifiersAreStableAndUnique() {
        let identifiers = MainWindowEntityName.allCases.map(\.accessibilityIdentifier)

        XCTAssertEqual(Set(identifiers).count, identifiers.count)
        XCTAssertEqual(
            MainWindowEntityName.tabs.accessibilityIdentifier,
            "actro.main-window.tabs"
        )
    }
}

import Foundation
import SwiftProtobuf
import XCTest

@testable import framework_codegen_swift

final class MetadataTests: XCTestCase {
  func testDispatchSpecJSONShapeRejectsUnknownFields() {
    let data = Data(#"{"schema_version":"v1","routes":[],"extra":true}"#.utf8)
    XCTAssertThrowsError(
      try ActrFrameworkGenerator.validateDispatchSpecJSONShape(data: data))
  }

  func testCanonicalJSONUsesJCSStringEscaping() throws {
    let separator = "\u{2028}"
    let canonical = try ActrFrameworkGenerator.canonicalizeJSONObject([
      "route": "local/\(separator)/é.proto"
    ])
    XCTAssertEqual(
      String(bytes: canonical, encoding: .utf8)!,
      "{\"route\":\"local/\(separator)/é.proto\"}"
    )
  }

  func testDispatchSpecValidationRejectsUnsupportedSchemaAndInvalidRoutes() throws {
    let baseRoute = ActrFrameworkGenerator.DispatchRoute(
      route_key: "echo.EchoService.Echo",
      proto_file: "remote/echo/echo.proto",
      proto_service: "echo.EchoService",
      method: "Echo",
      input_type: "echo.EchoRequest",
      output_type: "echo.EchoResponse",
      payload_type: "rpcReliable",
      dependency_alias: "echo"
    )

    XCTAssertThrowsError(
      try ActrFrameworkGenerator.validateDispatchSpec(
        .init(schema_version: "v2", routes: [], spec_hash: nil)
      ))
    XCTAssertThrowsError(
      try ActrFrameworkGenerator.validateDispatchSpec(
        .init(schema_version: "v1", routes: [baseRoute, baseRoute], spec_hash: nil)
      ))

    let emptyAliasRoute = ActrFrameworkGenerator.DispatchRoute(
      route_key: baseRoute.route_key,
      proto_file: baseRoute.proto_file,
      proto_service: baseRoute.proto_service,
      method: baseRoute.method,
      input_type: baseRoute.input_type,
      output_type: baseRoute.output_type,
      payload_type: baseRoute.payload_type,
      dependency_alias: " "
    )
    XCTAssertThrowsError(
      try ActrFrameworkGenerator.validateDispatchSpec(
        .init(schema_version: "v1", routes: [emptyAliasRoute], spec_hash: nil)
      ))
  }

  func testContractStructNameCombinesPackageServiceMethod() {
    XCTAssertEqual(
      ActrFrameworkGenerator.contractStructName(
        protoService: "echo.EchoService", method: "Echo"),
      "EchoEchoServiceEchoRpc")
    XCTAssertEqual(
      ActrFrameworkGenerator.contractStructName(
        protoService: "ask.AskService", method: "ContinuePrompt"),
      "AskAskServiceContinuePromptRpc")
  }

  func testPayloadTypeSwiftMapsKnownValues() throws {
    XCTAssertEqual(try ActrFrameworkGenerator.payloadTypeSwift("rpcReliable"), ".rpcReliable")
    XCTAssertEqual(try ActrFrameworkGenerator.payloadTypeSwift("rpcSignal"), ".rpcSignal")
    XCTAssertEqual(try ActrFrameworkGenerator.payloadTypeSwift("streamReliable"), ".streamReliable")
    XCTAssertEqual(try ActrFrameworkGenerator.payloadTypeSwift("streamLatencyFirst"), ".streamLatencyFirst")
    XCTAssertEqual(try ActrFrameworkGenerator.payloadTypeSwift("mediaRtp"), ".mediaRtp")
    XCTAssertThrowsError(try ActrFrameworkGenerator.payloadTypeSwift("unknown"))
  }

  func testNormalizeProtoPathHandlesBackslashesAndMissingSuffix() {
    XCTAssertEqual(
      ActrFrameworkGenerator.normalizeProtoPath(".\\remote\\ask"),
      "remote/ask.proto")
    XCTAssertEqual(
      ActrFrameworkGenerator.normalizeProtoPath("local/echo"),
      "local/echo.proto")
  }

  func testSnakeCaseAndLowerCamelCase() {
    XCTAssertEqual(ActrFrameworkGenerator.snakeCase("HTTPServer"), "http_server")
    XCTAssertEqual(ActrFrameworkGenerator.lowerCamelCase("HTTPServer"), "httpServer")
  }

  func testSwiftPackagePrefix() {
    XCTAssertEqual(ActrFrameworkGenerator.swiftPackagePrefix("echo"), "Echo_")
    XCTAssertEqual(ActrFrameworkGenerator.swiftPackagePrefix("data_stream_app"), "DataStreamApp_")
    XCTAssertEqual(ActrFrameworkGenerator.swiftPackagePrefix(""), "")
  }

  func testCollectSwiftTypeNamesResolvesNestedTypes() {
    var innerRequest = Google_Protobuf_DescriptorProto()
    innerRequest.name = "InnerRequest"
    var innerResponse = Google_Protobuf_DescriptorProto()
    innerResponse.name = "InnerResponse"
    var outer = Google_Protobuf_DescriptorProto()
    outer.name = "Outer"
    outer.nestedType = [innerRequest, innerResponse]

    var typeToSwiftName: [String: String] = [:]
    ActrFrameworkGenerator.collectSwiftTypeNames(
      package: "ask",
      message: outer,
      parentProtoName: nil,
      parentSwiftName: nil,
      into: &typeToSwiftName)

    XCTAssertEqual(typeToSwiftName["ask.Outer"], "Ask_Outer")
    XCTAssertEqual(typeToSwiftName["ask.Outer.InnerRequest"], "Ask_Outer.InnerRequest")
    XCTAssertEqual(typeToSwiftName["ask.Outer.InnerResponse"], "Ask_Outer.InnerResponse")
  }

  func testResolveSwiftTypeNameUsesDeclaringPackagePrefix() {
    let typeToSwiftName: [String: String] = [
      "echo.EchoRequest": "Echo_EchoRequest",
      "ask.Outer.InnerRequest": "Ask_Outer.InnerRequest",
    ]
    XCTAssertEqual(
      ActrFrameworkGenerator.resolveSwiftTypeName(
        ".echo.EchoRequest", typeToSwiftName: typeToSwiftName),
      "Echo_EchoRequest")
    XCTAssertEqual(
      ActrFrameworkGenerator.resolveSwiftTypeName(
        ".ask.Outer.InnerRequest", typeToSwiftName: typeToSwiftName),
      "Ask_Outer.InnerRequest")
    // Unknown types fall back to last component.
    XCTAssertEqual(
      ActrFrameworkGenerator.resolveSwiftTypeName(
        ".unknown.Type", typeToSwiftName: typeToSwiftName),
      "Type")
  }

  func testParseParametersParsesKeyValuePairs() {
    let params = ActrFrameworkGenerator.parseParameters(
      "Visibility=Public,dispatch_spec=/tmp/spec.json,dispatch_spec_hash=abc123,flag")
    XCTAssertEqual(params["Visibility"], "Public")
    XCTAssertEqual(params["dispatch_spec"], "/tmp/spec.json")
    XCTAssertEqual(params["dispatch_spec_hash"], "abc123")
    XCTAssertEqual(params["flag"], "")
  }

  func testRemoteDispatcherResolvesDependencyAliasAtRuntime() throws {
    let route = ActrFrameworkGenerator.DispatchRoute(
      route_key: "echo.EchoService.Echo",
      proto_file: "remote/echo/echo.proto",
      proto_service: "echo.EchoService",
      method: "Echo",
      input_type: "echo.EchoRequest",
      output_type: "echo.EchoResponse",
      payload_type: "rpcSignal",
      dependency_alias: "echo"
    )

    let source = try ActrFrameworkGenerator.generateUnifiedDispatcher(
      localRoutes: [],
      remoteRoutes: [route],
      localServiceNames: [],
      typeToSwiftName: [:]
    )

    XCTAssertTrue(source.contains("ctx.resolveDependency(alias: \"echo\")"))
    XCTAssertTrue(source.contains("ctx.discover(targetType: actrType)"))
    XCTAssertTrue(source.contains("ctx.callRaw("))
    XCTAssertTrue(source.contains("payloadType: .rpcSignal"))
    XCTAssertTrue(source.contains("timeoutMs: timeoutMs"))
    XCTAssertFalse(source.contains("remoteTargets"))
    XCTAssertTrue(source.contains("handler: any ActrDispatchHandler"))
  }

  func testLocalDispatcherRequiresUnifiedHandler() throws {
    let route = ActrFrameworkGenerator.DispatchRoute(
      route_key: "echo.EchoService.Echo",
      proto_file: "local/echo.proto",
      proto_service: "echo.EchoService",
      method: "Echo",
      input_type: "echo.EchoRequest",
      output_type: "echo.EchoResponse",
      payload_type: "rpcReliable",
      dependency_alias: nil
    )

    let source = try ActrFrameworkGenerator.generateUnifiedDispatcher(
      localRoutes: [route],
      remoteRoutes: [],
      localServiceNames: ["echo.EchoService"],
      typeToSwiftName: [:]
    )

    XCTAssertTrue(source.contains("public protocol ActrDispatchHandler: EchoServiceHandler, Sendable"))
    XCTAssertTrue(source.contains("handler: any ActrDispatchHandler"))
  }

  func testMetadataMatchesCliSchemaAndCarriesAlias() throws {
    let remoteRoute = ActrFrameworkGenerator.DispatchRoute(
      route_key: "echo.EchoService.Echo",
      proto_file: "remote/echo/echo.proto",
      proto_service: "echo.EchoService",
      method: "Echo",
      input_type: "echo.EchoRequest",
      output_type: "echo.EchoResponse",
      payload_type: "rpcReliable",
      dependency_alias: "echo"
    )
    let localRoute = ActrFrameworkGenerator.DispatchRoute(
      route_key: "local.ProbeService.Start",
      proto_file: "local/probe.proto",
      proto_service: "local.ProbeService",
      method: "Start",
      input_type: "local.StartRequest",
      output_type: "local.StartResponse",
      payload_type: "rpcReliable",
      dependency_alias: nil
    )
    let typeRefs: [String: ActrFrameworkGenerator.TypeRef] = [
      "local.StartRequest": .init(
        proto_type: "local.StartRequest",
        type_name: "StartRequest",
        proto_package: "local",
        proto_file: "local/probe.proto"
      ),
      "local.StartResponse": .init(
        proto_type: "local.StartResponse",
        type_name: "StartResponse",
        proto_package: "local",
        proto_file: "local/probe.proto"
      ),
    ]
    let metadata = ActrFrameworkGenerator.ActrGenMetadata(
      schema_version: "v1",
      plugin_version: "0.6.1",
      language: "swift",
      dispatch_spec_hash: "abc123",
      generated_files: ["actr_dispatch.swift", "actr-gen-meta.json"],
      generated_local_routes: [localRoute.route_key],
      generated_remote_routes: [remoteRoute.route_key],
      local_services: ActrFrameworkGenerator.buildLocalServiceMetadata(
        [localRoute], typeRefs: typeRefs),
      remote_services: ActrFrameworkGenerator.buildRemoteServiceMetadata(
        [remoteRoute], typeRefs: [:])
    )

    let object = try XCTUnwrap(
      JSONSerialization.jsonObject(with: JSONEncoder().encode(metadata)) as? [String: Any]
    )
    XCTAssertEqual(object["plugin_version"] as? String, "0.6.1")
    XCTAssertEqual(object["language"] as? String, "swift")
    XCTAssertEqual(object["schema_version"] as? String, "v1")
    XCTAssertEqual(object["dispatch_spec_hash"] as? String, "abc123")
    XCTAssertEqual(
      object["generated_files"] as? [String],
      ["actr_dispatch.swift", "actr-gen-meta.json"])
    let localServices = try XCTUnwrap(object["local_services"] as? [[String: Any]])
    XCTAssertEqual(localServices.first?["handler_interface"] as? String, "ProbeServiceHandler")
    XCTAssertEqual(localServices.first?["dispatcher_type"] as? String, "ProbeServiceDispatcher")
    XCTAssertEqual(localServices.first?["workload_type"] as? String, "")
    let localMethods = try XCTUnwrap(localServices.first?["methods"] as? [[String: Any]])
    let inputRef = try XCTUnwrap(localMethods.first?["input_ref"] as? [String: Any])
    XCTAssertEqual(inputRef["proto_file"] as? String, "local/probe.proto")
    let remoteServices = try XCTUnwrap(object["remote_services"] as? [[String: Any]])
    XCTAssertEqual(remoteServices.first?["dependency_alias"] as? String, "echo")
    XCTAssertNil(remoteServices.first?["actr_type"])
  }
}

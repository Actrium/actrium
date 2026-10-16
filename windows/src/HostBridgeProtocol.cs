using System.Text.Json;
using System.Text.Json.Serialization;

namespace Actrium.Windows;

internal sealed class HostBridgeDispatcher
{
    public const string RequestType = "actro.host.request";
    public const string ResponseType = "actro.host.response";
    public const string GetCoreStatusMethod = "actro.host.getCoreStatus";
    public const string OpenTabMethod = "actro.host.openTab";
    public const string CloseTabMethod = "actro.host.closeTab";
    public const string SelectTabMethod = "actro.host.selectTab";
    public const string NavigateMethod = "actro.host.navigate";
    public const string GetTabsMethod = "actro.host.getTabs";
    public const string GetActiveTabMethod = "actro.host.getActiveTab";
    public const string RemoveBookmarkMethod = "actro.host.removeBookmark";
    public const string RemoveHistoryEntryMethod = "actro.host.removeHistoryEntry";

    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
    };

    private readonly Guid ownerTabId;
    private readonly HostBridgeActions actions;
    private readonly Func<string, bool> isSourceAllowed;

    public HostBridgeDispatcher(
        Guid ownerTabId,
        HostBridgeActions actions,
        Func<string, bool>? isSourceAllowed = null)
    {
        this.ownerTabId = ownerTabId;
        this.actions = actions;
        this.isSourceAllowed = isSourceAllowed ?? IsDefaultAllowedSource;
    }

    internal async Task<string> HandleMessageJsonAsync(string messageJson, string source = "")
    {
        string? requestId = null;
        try
        {
            var request = HostBridgeRequest.Parse(messageJson);
            requestId = request.Id;
            if (request.Type != RequestType)
            {
                return SerializeError(request.Id, "invalid_type", "Unsupported Actrium host bridge message type.");
            }

            if (!isSourceAllowed(source))
            {
                return SerializeError(request.Id, "forbidden_source", "This document is not allowed to use the Actrium host bridge.");
            }

            return request.Method switch
            {
                GetCoreStatusMethod => SerializeSuccess(
                    request.Id,
                    ActriumCoreStatusPayload.From(actions.LoadCoreStatus())),
                OpenTabMethod => SerializeSuccess(
                    request.Id,
                    await actions.OpenTabAsync(
                        ownerTabId,
                        DeserializeParams(request.Params, new HostOpenTabRequest()))),
                CloseTabMethod => SerializeSuccess(
                    request.Id,
                    await actions.CloseTabAsync(
                        ownerTabId,
                        DeserializeParams(request.Params, new HostCloseTabRequest()))),
                SelectTabMethod => SerializeSuccess(
                    request.Id,
                    await actions.SelectTabAsync(
                        DeserializeParams(request.Params, new HostSelectTabRequest()))),
                NavigateMethod => SerializeSuccess(
                    request.Id,
                    await actions.NavigateAsync(
                        ownerTabId,
                        DeserializeParams(request.Params, new HostNavigateRequest()))),
                GetTabsMethod => SerializeSuccess(request.Id, actions.GetTabs()),
                GetActiveTabMethod => SerializeSuccess(request.Id, actions.GetActiveTab()),
                RemoveBookmarkMethod => SerializeSuccess(
                    request.Id,
                    await actions.RemoveBookmarkAsync(
                        DeserializeParams(request.Params, new HostRemoveBookmarkRequest()))),
                RemoveHistoryEntryMethod => SerializeSuccess(
                    request.Id,
                    await actions.RemoveHistoryEntryAsync(
                        DeserializeParams(request.Params, new HostRemoveHistoryEntryRequest()))),
                _ => SerializeError(request.Id, "unknown_method", "Unsupported Actrium host bridge method."),
            };
        }
        catch (HostBridgeException error)
        {
            return SerializeError(requestId, error.Code, error.Message);
        }
        catch (JsonException error)
        {
            return SerializeError(requestId, "invalid_json", error.Message);
        }
        catch (Exception error)
        {
            return SerializeError(requestId, "internal_error", error.Message);
        }
    }

    internal static bool IsDefaultAllowedSource(string source)
    {
        if (string.IsNullOrWhiteSpace(source))
        {
            return true;
        }

        if (!Uri.TryCreate(source, UriKind.Absolute, out var uri))
        {
            return false;
        }

        if (uri.Scheme is "about" or "data" || uri.Scheme == Uri.UriSchemeFile)
        {
            return true;
        }

        return uri.IsLoopback
            && (uri.Scheme == Uri.UriSchemeHttp || uri.Scheme == Uri.UriSchemeHttps);
    }

    private static string SerializeSuccess(string? id, object? result)
    {
        return JsonSerializer.Serialize(
            new HostBridgeResponse(ResponseType, id, true, result, null),
            JsonOptions);
    }

    private static string SerializeError(string? id, string code, string message)
    {
        return JsonSerializer.Serialize(
            new HostBridgeResponse(ResponseType, id, false, null, new HostBridgeError(code, message)),
            JsonOptions);
    }

    private static T DeserializeParams<T>(JsonElement? paramsElement, T defaultValue)
    {
        if (paramsElement is null)
        {
            return defaultValue;
        }

        if (paramsElement.Value.ValueKind != JsonValueKind.Object)
        {
            throw new JsonException("Actrium host bridge params must be a JSON object.");
        }

        return paramsElement.Value.Deserialize<T>(JsonOptions) ?? defaultValue;
    }
}

internal sealed record HostBridgeRequest(string? Type, string? Id, string? Method, JsonElement? Params)
{
    public static HostBridgeRequest Parse(string messageJson)
    {
        using var document = JsonDocument.Parse(messageJson);
        if (document.RootElement.ValueKind != JsonValueKind.Object)
        {
            throw new JsonException("Actrium host bridge messages must be JSON objects.");
        }

        return new HostBridgeRequest(
            ReadOptionalString(document.RootElement, "type"),
            ReadOptionalString(document.RootElement, "id"),
            ReadOptionalString(document.RootElement, "method"),
            ReadOptionalParams(document.RootElement));
    }

    private static string? ReadOptionalString(JsonElement root, string propertyName)
    {
        if (!root.TryGetProperty(propertyName, out var property) || property.ValueKind == JsonValueKind.Null)
        {
            return null;
        }

        if (property.ValueKind != JsonValueKind.String)
        {
            throw new JsonException($"Actrium host bridge property '{propertyName}' must be a string.");
        }

        return property.GetString();
    }

    private static JsonElement? ReadOptionalParams(JsonElement root)
    {
        if (!root.TryGetProperty("params", out var property) || property.ValueKind == JsonValueKind.Null)
        {
            return null;
        }

        return property.Clone();
    }
}

internal sealed record HostBridgeResponse(
    string Type,
    string? Id,
    bool Ok,
    object? Result,
    HostBridgeError? Error);

internal sealed record HostBridgeError(string Code, string Message);

internal sealed class HostBridgeException : Exception
{
    public HostBridgeException(string code, string message)
        : base(message)
    {
        Code = code;
    }

    public string Code { get; }
}

internal sealed record HostBridgeActions(
    Func<ActriumCoreStatus> LoadCoreStatus,
    Func<Guid, HostOpenTabRequest, Task<HostOpenTabPayload>> OpenTabAsync,
    Func<Guid, HostCloseTabRequest, Task<bool>> CloseTabAsync,
    Func<HostSelectTabRequest, Task<bool>> SelectTabAsync,
    Func<Guid, HostNavigateRequest, Task<HostTabPayload>> NavigateAsync,
    Func<HostTabsPayload> GetTabs,
    Func<HostTabPayload?> GetActiveTab,
    Func<HostRemoveBookmarkRequest, Task<bool>> RemoveBookmarkAsync,
    Func<HostRemoveHistoryEntryRequest, Task<bool>> RemoveHistoryEntryAsync);

internal sealed record HostOpenTabRequest
{
    public string? Url { get; init; }

    public string? Address { get; init; }

    public string? AppUrl { get; init; }

    public string? Title { get; init; }

    public bool? Activate { get; init; }
}

internal sealed record HostCloseTabRequest
{
    public string? TabId { get; init; }
}

internal sealed record HostSelectTabRequest
{
    public string? TabId { get; init; }
}

internal sealed record HostNavigateRequest
{
    public string? TabId { get; init; }

    public string? Url { get; init; }

    public string? Address { get; init; }
}

internal sealed record HostRemoveBookmarkRequest
{
    public string? Url { get; init; }

    public string? Address { get; init; }
}

internal sealed record HostRemoveHistoryEntryRequest
{
    public string? Url { get; init; }

    public string? Address { get; init; }
}

internal sealed record HostOpenTabPayload(string TabId, HostTabPayload Tab);

internal sealed record HostTabsPayload(string? ActiveTabId, IReadOnlyList<HostTabPayload> Tabs);

internal sealed record HostTabPayload(
    string TabId,
    string Title,
    string Address,
    bool IsActive,
    bool IsLoading,
    bool CanGoBack,
    bool CanGoForward);

internal sealed record ActriumCoreStatusPayload(
    bool IsAvailable,
    string ProductName,
    uint CApiVersion,
    string SupportedContractName,
    uint SupportedContractVersion,
    uint CoreSchemaVersion,
    uint RuntimeSchemaVersion,
    string? UnavailableReason)
{
    public static ActriumCoreStatusPayload From(ActriumCoreStatus status)
    {
        return new ActriumCoreStatusPayload(
            status.IsAvailable,
            status.ProductName,
            status.CApiVersion,
            status.SupportedContractName,
            status.SupportedContractVersion,
            status.CoreSchemaVersion,
            status.RuntimeSchemaVersion,
            status.UnavailableReason);
    }
}

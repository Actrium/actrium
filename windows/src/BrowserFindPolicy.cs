using System.Text.Json;

namespace Actro.Windows;

internal static class BrowserFindPolicy
{
    public static string BuildWindowFindScript(string query, bool forward)
    {
        var encodedQuery = JsonSerializer.Serialize(query);
        var backwards = forward ? "false" : "true";
        return $"window.find({encodedQuery}, false, {backwards}, true, false, true, false);";
    }

    public static bool IsFoundResult(string scriptResult)
    {
        return string.Equals(scriptResult.Trim(), "true", StringComparison.OrdinalIgnoreCase);
    }
}

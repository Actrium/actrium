using System.Collections.Generic;

namespace Actrium.Windows;

internal sealed record WindowsStartupOptions(
    IReadOnlyList<string> InitialAddresses,
    bool IsPrivate = false,
    string? UserDataRoot = null)
{
    public WindowsStartupOptions(string? initialUrl, bool isPrivate = false)
        : this(
            string.IsNullOrWhiteSpace(initialUrl) ? [] : [initialUrl],
            isPrivate)
    {
    }

    public string? InitialUrl => InitialAddresses.FirstOrDefault();

    public static WindowsStartupOptions FromArgs(IReadOnlyList<string> args)
    {
        var initialAddresses = new List<string>();
        var isPrivate = false;
        string? userDataRoot = null;

        for (var index = 0; index < args.Count; index++)
        {
            if (args[index] == "--url" && index + 1 < args.Count)
            {
                initialAddresses.Add(args[index + 1]);
                index++;
                continue;
            }

            // Concurrent multi-instance runs: overrides the durable state
            // root so a second instance keeps a fully independent state tree.
            // Program.Main mirrors it into ACTRIUM_USER_DATA_ROOT, the single
            // override choke point WindowsAppPaths reads.
            if (args[index] == "--user-data-root" && index + 1 < args.Count)
            {
                userDataRoot = args[index + 1];
                index++;
                continue;
            }

            if (args[index] is "--private" or "--private-window" or "--incognito")
            {
                isPrivate = true;
                continue;
            }

            if (!args[index].StartsWith("-", StringComparison.Ordinal)
                && !string.IsNullOrWhiteSpace(args[index]))
            {
                initialAddresses.Add(args[index]);
            }
        }

        return new WindowsStartupOptions(initialAddresses, isPrivate, userDataRoot);
    }
}

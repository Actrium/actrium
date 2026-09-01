namespace Actro.Windows;

internal sealed class BrowserClosedTabStack
{
    private const int MaxClosedTabs = 25;

    private readonly List<BrowserSessionTabSnapshot> snapshots = [];

    public bool CanReopen => snapshots.Count > 0;

    public int Count => snapshots.Count;

    public void Push(BrowserSessionTabSnapshot snapshot)
    {
        var normalized = BrowserSessionStore.NormalizeTab(snapshot);
        if (normalized is null)
        {
            return;
        }

        snapshots.Add(normalized);
        if (snapshots.Count > MaxClosedTabs)
        {
            snapshots.RemoveRange(0, snapshots.Count - MaxClosedTabs);
        }
    }

    public BrowserSessionTabSnapshot? Pop()
    {
        if (snapshots.Count == 0)
        {
            return null;
        }

        var index = snapshots.Count - 1;
        var snapshot = snapshots[index];
        snapshots.RemoveAt(index);
        return snapshot;
    }
}

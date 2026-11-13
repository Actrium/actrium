"""Validate selected (binary-id, test-name) identities, not only counts."""
import json
import sys


def selected(inventory):
    items = []
    for suite in inventory['rust-suites'].values():
        for name, case in suite.get('testcases', {}).items():
            if not case['ignored'] and case['filter-match']['status'] == 'matches':
                items.append((suite['binary-id'], name))
    if len(items) != len(set(items)):
        raise ValueError('duplicate selected test identities')
    return set(items)


def verify(whole, *partitions):
    if not whole or not partitions or any(not part for part in partitions):
        raise ValueError('all test partitions must be nonempty')
    combined = set()
    for part in partitions:
        overlap = combined & part
        if overlap:
            raise ValueError(f'overlapping tests: {sorted(overlap)}')
        combined |= part
    if combined != whole:
        raise ValueError(f'missing={sorted(whole - combined)}; extra={sorted(combined - whole)}')
    print(f'Partition verified: whole={len(whole)} partitions={[len(part) for part in partitions]}')


if __name__ == '__main__':
    if len(sys.argv) < 4:
        raise SystemExit('expected whole and at least two partition JSON files')
    inventories = []
    for path in sys.argv[1:]:
        with open(path) as inventory:
            inventories.append(selected(json.load(inventory)))
    verify(*inventories)

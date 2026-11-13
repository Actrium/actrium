"""Use broad runner cleanup when free disk is below the build threshold."""
import argparse
import os
import shutil

MIN_FREE_BYTES = 10 * 1024**3


def cleanup_needed(available_bytes, minimum_bytes=MIN_FREE_BYTES):
    if available_bytes < 0:
        raise ValueError('invalid available space')
    return available_bytes < minimum_bytes


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--path', default='.')
    parser.add_argument('--minimum-free-gib', type=int, default=MIN_FREE_BYTES // 1024**3)
    args = parser.parse_args()
    if args.minimum_free_gib <= 0:
        parser.error('--minimum-free-gib must be positive')
    minimum_bytes = args.minimum_free_gib * 1024**3
    free = shutil.disk_usage(args.path).free
    needed = cleanup_needed(free, minimum_bytes)
    print(f'Available {free} bytes; minimum {minimum_bytes}; cleanup_needed={needed}')
    with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
        output.write(f'cleanup_needed={str(needed).lower()}\n')


if __name__ == '__main__':
    main()

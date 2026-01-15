#!/bin/bash -eu
#
# Clean cache and reinstall the project dependent libraries.

if [[ -d ./build ]]; then
  printf 'Cleaning build caches...'
  rm -rf ./build
  printf '\t✅\n'
fi

if [[ -d ~/Library/Developer/Xcode/DerivedData ]]; then
  printf 'Cleaning Xcode caches...'
  rm -rf ~/Library/Developer/Xcode/DerivedData/
  printf '\t✅\n'
fi

if [[ -d ./Pods ]]; then
  printf 'Cleaning CocoPods files...'
  rm -rf ./Pods
  printf '\t✅\n'
fi

if [[ -d ./Subsystem/node_modules ]]; then
  printf 'Cleaning bun files...'
  rm -rf ./Subsystem/node_modules
  rm -rf ./Subsystem/bun.lock
  printf '\t\t✅\n'
fi

printf '\n\n\33[4;32m%s\33[0m\n' 'Reinstalling node modules...'
printf '=============================\n'
cd Subsystem && bun install

printf '\n\n\33[4;32m%s\33[0m\n' 'Copying modified node modules...'
printf '=============================\n'
cp -rvf ./modify_modules/* ./node_modules/

printf '\n\n\33[4;32m%s\33[0m\n' 'Reinstalling CocoPods files...'
printf '=============================\n'
cd - || exit 1
pod install

printf '\33[1;32m%s\33[0m\n' 'Done!'
exit 0

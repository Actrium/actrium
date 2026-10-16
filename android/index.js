import React from 'react';
import { AppRegistry } from 'react-native';
import { name as appName } from './app.json';
import './actrium-runtime/src/ensure-console-assert';
import RuntimeApp from './actrium-runtime';

AppRegistry.registerComponent(appName, () => RuntimeApp);

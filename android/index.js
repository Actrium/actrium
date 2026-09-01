import React from 'react';
import { AppRegistry } from 'react-native';
import { name as appName } from './app.json';
import './actro-runtime/src/ensure-console-assert';
import RuntimeApp from './actro-runtime';

AppRegistry.registerComponent(appName, () => RuntimeApp);

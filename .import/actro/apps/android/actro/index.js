import React from 'react';
import { AppRegistry, View, Text } from 'react-native';
import { name as appName } from './app.json';
import './loom/src/ensure-console-assert';
// import App from './App';
// import { ActrApp } from './Subsystem';
// import { ActrApp } from 'canvas-subsystem';
import ActrApp from './loom';

AppRegistry.registerComponent(appName,()=> ActrApp);

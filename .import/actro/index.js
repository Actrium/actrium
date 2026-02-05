import React from 'react';
import { AppRegistry, View, Text } from 'react-native';
import { name as appName } from './app.json';
import App from './App';
import { ActrApp } from './Subsystem/index.ts';


AppRegistry.registerComponent(appName,()=> ActrApp);
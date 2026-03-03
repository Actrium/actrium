import React from 'react';
import { AppRegistry, View, Text } from 'react-native';
import { name as appName } from './app.json';
// import App from './App';
// import { ActrApp } from './Subsystem';
// import { ActrApp } from 'canvas-subsystem';
import ActrApp from './loom';

console.log("--------------------start", globalThis.__QuickJSSandboxJSI)
console.log('global keys include sandbox?', Object.keys(globalThis).includes('__QuickJSSandboxJSI'));
// const runtime = __QuickJSSandboxJSI.createRuntime({ timeout: 5000 });
// const available = __QuickJSSandboxJSI.isAvailable();

// console.log("index",runtime);
// console.log("index", available)
console.log("--------------------end")

AppRegistry.registerComponent(appName,()=> ActrApp);
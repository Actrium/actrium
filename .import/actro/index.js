/**
 * @format
 */
import React from 'react';
import { AppRegistry, View, Text } from 'react-native';
import { name as appName } from './app.json';
// 临时使用简化版本进行测试（不带扩展名）
import { ActrApp } from './Subsystem/index';


AppRegistry.registerComponent(appName,()=> ActrApp);

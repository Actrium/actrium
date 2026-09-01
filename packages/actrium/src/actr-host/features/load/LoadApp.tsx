import React from 'react';
import { isPhone } from '../../../utils/device';
import LoadAppPhone from './LoadAppPhone';
import LoadAppPad from './LoadAppPad';

const LoadApp = () => {
  if (isPhone) {
    return <LoadAppPhone />;
  }
  return <LoadAppPad />;
};

export default LoadApp;

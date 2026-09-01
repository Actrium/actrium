import { Text, TouchableOpacity, View } from 'rill/let';
import { getRandom } from '../utils';
import { useState } from 'react';

const Tmp = () => {
  const [num, setNum] = useState(0);

  return (
    <View>
      <Text style={{ color: '#f40', fontSize: 20 }}>
        引用组件 =  {getRandom()}
      </Text>

      <TouchableOpacity
        onPress={() => {
          setNum(num + 1);
        }}
      >
        <Text style={{ color: '#f40', fontSize: 20 }}>{num}</Text>
      </TouchableOpacity>
    </View>
  );
};

export default Tmp;

import { Text, TouchableOpacity, View } from 'keel/guest';
import { getRandom } from '../utils';
import { useState } from 'react';

const Tmp = () => {
  const [num, setNum] = useState(0);

  return (
    <View>
      <Text style={{ color: '#f40', fontSize: 20 }}>
        Referenced component = {getRandom()}
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

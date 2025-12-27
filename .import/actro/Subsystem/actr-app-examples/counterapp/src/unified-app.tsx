/**
 * Unified App - Single Bundle Architecture
 * 统一应用入口：单个 bundle 包含左右面板
 *
 * 架构说明：
 * - 使用 askit 提供的 Panel DSL 标记面板
 * - Host 通过 extractPanels() 提取对应面板
 * - 跨面板交互：左侧按钮点击会更新右侧历史记录
 *
 * @version 2.1.0 - Add: Cross-panel interaction
 */
import React, { useState } from 'react';
import { View } from 'rill/let';
import { Panel } from 'askit';
import LeftPanel from './left-panel';
import RightPanel from './right-panel';

export interface ClickRecord {
  id: number;
  time: string;
  action: string;
  emoji: string;
  color: string;
}

export default function UnifiedApp() {
  // 共享状态：跨面板的点击历史和统计
  const [clickHistory, setClickHistory] = useState<ClickRecord[]>([]);
  const [sessionClicks, setSessionClicks] = useState(0);

  // 添加点击记录的统一函数
  const addRecord = (action: string, emoji: string, color: string) => {
    const now = new Date();
    const timeStr = `${now.getHours().toString().padStart(2, '0')}:${now.getMinutes().toString().padStart(2, '0')}:${now.getSeconds().toString().padStart(2, '0')}`;

    const newRecord: ClickRecord = {
      id: Date.now(),
      time: timeStr,
      action,
      emoji,
      color
    };

    setClickHistory([newRecord, ...clickHistory].slice(0, 20)); // 保留最近20条
    setSessionClicks(sessionClicks + 1);
  };

  // 清空历史记录
  const clearHistory = () => {
    setClickHistory([]);
    setSessionClicks(0);
  };

  return (
    <View style={{ flex: 1 }}>
      {/* 左侧面板容器 - 传递 addRecord 函数实现跨面板交互 */}
      <Panel.Left>
        <LeftPanel onAction={addRecord} />
      </Panel.Left>

      {/* 右侧面板容器 - 传递共享状态 */}
      <Panel.Right>
        <RightPanel
          clickHistory={clickHistory}
          sessionClicks={sessionClicks}
          onAddRecord={addRecord}
          onClearHistory={clearHistory}
        />
      </Panel.Right>
    </View>
  );
}

export type Overview<T, N extends keyof T, D> = Omit<T, N> & Record<N, D>;

export interface PresetInfo {
  language: string;
  description: string;
  welcome_text: string;
  title: string;
  default_questions: {
    question: string;
    answer: string;
  }[];
}

export interface AppInfo {
  app_id: string;
  app_status?: 'alive' | 'dead';
  manifest_version: string; // 配置文件版本
  app_name: string; // 应用名称
  instance_name: string; // 实例名称

  actr_url: string; // guest cdn url
  actr_hash_code: string; // guest sha1 code
  realm_id: string; // toml realm id
  server_actor_type: {
    manufacturer: string; // toml actor manufacturer
    name: string; // toml actor name
  };

  info: {
    logo: string;
    background_image: string;
    default_language: string;
  };
  client: {
    enable_search: boolean;
    send_on_enter: boolean;
    share_location: boolean;
  };
  client_id: string;
  client_secret: string;
  allow_anonymous_access: boolean; // 是否允许匿名访问
  languages: string[];
  language_contents: Record<string, PresetInfo>;
  knowledge_base: string[]; // 关联知识库id
  engine: 'aliyun'; // 引擎
  settings: {
    auto_tts_enabled: boolean; //答案语音自动播放
    dialogue_limit_per_day: number; //限制用户每日对话次数（如设为 100，即每天最多 100 次问答）
    custom_theme: {
      primary_color: string; //应用主色调（用于按钮、标题等主视觉色）示例：#1976D2 是蓝色
      primary_color_dark: string;
    };
  };
  status: string; // 状态
  key_id: string;
  authority_services: {
    description: string;
    domain_name: string;
    name: string;
    port_info: string;
    service_type: string;
    status: string;
  }[];
  signaling_services: {
    description: string;
    domain_name: string;
    name: string;
    port_info: string;
    service_type: string;
    status: string;
  }[];
  stun_services: {
    description: null;
    domain_name: string;
    name: string;
    port_info: string;
    service_type: string;
    status: string;
  }[];
  turn_services: {
    description: string;
    domain_name: string;
    name: string;
    port_info: string;
    service_type: string;
    status: string;
  }[];
  creator_id: string; // 创建人、租户
  creator_id_transfer: string; // 创建人、租户姓名
  created_at: string; // 创建时间
  updated_at: string; // 最后更新时间
  url?: string; // 应用加载来源地址 (扫码或手动输入的 URL)
}

export interface AuthAppInfo {
  scopes: string[];
  app: {
    id: string;
    name: string;
    description: string;
    kind: string;
    identifier: string;
    protocol: string;
    client_id: string;
    client_secret: string;
    home: string;
    callback: string;
    created_at: string;
    updated_at: string;
  };
}

export interface AccountUser {
  dn: string;
  cn: string;
  display_name: string;
  avatar: string;
  telephone_number: string;
  mail: string;
  user_password?: string;
  created_via: string;
  person_status: string;
  profiles: string[];
  mfa_options: any[];
  assigned_policy: any[];
  created_at: string;
  updated_at: string;
  member_of: any[];
}

export interface AccountProfile {
  dn: string;
  cn: string;
  id: string;
  domain: string;
  display_name: string;
  mail: string;
  person_status: string;
  member: any[];
  created_at: string;
  updated_at: string;
}

export interface AccountUserInfo {
  user: AccountUser;
  profiles: AccountProfile[];
  mfas: any[];
}

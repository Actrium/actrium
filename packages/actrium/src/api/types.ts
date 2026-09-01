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
  manifest_version: string; // Profile Version
  app_name: string; // Apply Name
  instance_name: string; // Example Name

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
  allow_anonymous_access: boolean; // Anonymous access allowed
  languages: string[];
  language_contents: Record<string, PresetInfo>;
  knowledge_base: string[]; // Associated knowledge baseid
  engine: 'aliyun'; // Engine
  settings: {
    auto_tts_enabled: boolean; //Answers automatically play
    dialogue_limit_per_day: number; //Limit the number of daily conversations between users (if any) 100, or maximum per day 100 (Questions and answers)
    custom_theme: {
      primary_color: string; //Example:#1976D2 It's blue.
      primary_color_dark: string;
    };
  };
  status: string; // Status
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
  creator_id: string; // Founder, Tenant
  creator_id_transfer: string; // Name of the founder, tenant
  created_at: string; // Created
  updated_at: string; // Last update time
  url?: string; // Apply Load Source Address (Scanning or manual input URL)
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

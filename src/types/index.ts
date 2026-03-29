export interface Runner {
  id: string;
  name: string;
  path: string;
  type: string;
}

export interface AppConfig {
  username: string;
  linuxRunner?: string;
}

export interface InstalledStatus {
  lcre_nightly: boolean;
  vanilla_tu19: boolean;
  vanilla_tu24: boolean;
  [key: string]: boolean;
}

export interface ReinstallModalData {
  id: string;
  url: string;
  isUpdate?: boolean;
}

export interface McNotification {
  t: string;
  m: string;
}

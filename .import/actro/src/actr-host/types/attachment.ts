export interface IAttachmentItem {
  id: string;
  attachment_type: 'image' | 'file' | 'location'; // 内部区分类型
  local_url: string;
  // 图片特有
  cdn_url?: string;
  aspect_ratio?: number;
  // 文件特有
  file_type?: string;
  file_name?: string;
  // 地点特有
  latitude?: number;
  longitude?: number;
  place_name?: string;
}

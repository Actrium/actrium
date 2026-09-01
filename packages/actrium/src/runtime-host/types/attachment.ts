export interface IAttachmentItem {
  id: string;
  attachment_type: 'image' | 'file' | 'location'; // Internal Distinction Type
  local_url: string;
  // Picture-specific
  cdn_url?: string;
  aspect_ratio?: number;
  // Document-specific
  file_type?: string;
  file_name?: string;
  // Location-specific
  latitude?: number;
  longitude?: number;
  place_name?: string;
}

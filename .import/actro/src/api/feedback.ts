import { CONSOLE_BASE } from './app';
import { http } from './https';

export interface FeedbackContext {
  role: 'user' | 'assistant';
  message_id: string;
  content: string;
  created_at: string;
}

export interface FeedbackPayload {
  app_id: string;
  user_id: string;
  type: string[];
  note: string;
  context: FeedbackContext[];
  message_id: string;
}

export const submitFeedback = async (payload: FeedbackPayload) => {
  return await http.post<any>(`${CONSOLE_BASE}/api/feedbacks`, payload);
};

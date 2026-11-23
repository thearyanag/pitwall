/** Implemented by native drivers and the shared ACP driver. No Board/UI imports. */
export interface AgentInstance {
  id: string;
  label: string;
  driver: string;
  command: string;
  args: string[];
  path: string;
  env?: Record<string, string>;
  model?: string;
}
export interface Capabilities {
  images: boolean;
  resume: boolean;
}
export interface ModelChoice {
  id: string;
  label: string;
  description?: string;
  images?: boolean;
}
export interface AgentHost {
  signal: AbortSignal;
  emit(event: { type: string; [key: string]: unknown }): void;
  ask(
    title: string,
    detail: string,
    options: { id: string; label: string }[],
  ): Promise<string | null>;
}
export interface SessionOptions {
  cwd: string;
  resumeId?: string;
  model?: string | null;
  defaultModel?: string | null;
  discover?: boolean;
}
export interface AgentSession {
  id: string | null;
  resumed: boolean;
  capabilities: Capabilities;
  models: ModelChoice[];
  model: string | null;
  defaultModel: string | null;
  prompt(input: {
    text: string;
    images: { path: string; type: string }[];
  }): Promise<string>;
  cancel(): Promise<void>;
  close(): Promise<void>;
}
export type OpenAgent = (
  instance: AgentInstance,
  options: SessionOptions,
  host: AgentHost,
) => Promise<AgentSession>;

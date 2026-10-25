/** Shared pure WebAudio graph assembly. No project or native host state. */
export const CLASSIC_MASTERING_LOOKAHEAD_SECONDS: number;
export function createClassicMasteringChain(context: BaseAudioContext, destination: AudioNode): {
  input: GainNode;
  disconnect(): void;
};

/** Shared pure WebAudio graph assembly. No project or native host state. */
export function createClassicMasteringChain(context: BaseAudioContext, destination: AudioNode): {
  input: GainNode;
  disconnect(): void;
};

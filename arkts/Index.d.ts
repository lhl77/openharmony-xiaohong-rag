export interface Envelope<T> {
  ok: boolean;
  data?: T;
  error?: { code: string; message: string };
}
export interface DocumentInput {
  documentId: string; revision: number; title: string; course: string; text: string;
}
export interface SearchRequest {
  query: string; courseFilter?: string; topK?: number; rrfK?: number;
  vectorWeight?: number; keywordWeight?: number;
}
export interface SourceReference { documentId: string; revision: number; chunkId: string; }
export interface StyleRequest { allowedDocumentIds: string[]; }
export interface Citation {
  documentId: string; revision: number; title: string; course: string; chunkId: string;
  sourceStart: number; sourceEnd: number; text: string;
}
export interface SearchHit {
  citation: Citation; score: number; vectorRank: number | null; keywordRank: number | null;
}
export interface SearchResponse {
  schemaVersion: number; query: string; hits: SearchHit[]; warnings: string[];
}
export interface StyleProfile {
  headingLevels: number[]; orderedList: boolean; bullets: string[];
  averageParagraphLength: number;
}
export interface IndexStats { documents: number; chunks: number; embeddingDimension: number; }
export interface InitConfig {
  chunkTargetChars?: number; chunkOverlapChars?: number;
  embeddingDimension?: number; minimumChunkChars?: number;
}

declare const rag: {
  init(requestJson: string): Promise<string>;
  addDocuments(requestJson: string): Promise<string>;
  search(requestJson: string): Promise<string>;
  searchScoped(requestJson: string): Promise<string>;
  resolve(requestJson: string): Promise<string>;
  resolveScoped(requestJson: string): Promise<string>;
  styleProfile(requestJson: string): Promise<string>;
  removeDocument(requestJson: string): Promise<string>;
};
export default rag;export interface Citation { documentId:string; revision:number; title:string; course:string; chunkId:string; sourceStart:number; sourceEnd:number; text:string; }
export interface SearchHit { citation:Citation; score:number; vectorRank?:number; keywordRank?:number; }
export interface SearchResponse { schemaVersion:number; query:string; hits:SearchHit[]; warnings:string[]; }
export interface SourceReference { documentId:string; revision:number; chunkId:string; }
export interface StyleProfile { headingLevels:number[]; orderedList:boolean; bullets:string[]; averageParagraphLength:number; }
declare const rag:{
	init(configJson:string):Promise<string>;
	addDocuments(requestJson:string):Promise<string>;
	search(requestJson:string):Promise<string>;
	searchScoped(requestJson:string):Promise<string>;
	resolve(requestJson:string):Promise<string>;
	resolveScoped(requestJson:string):Promise<string>;
	styleProfile(requestJson:string):Promise<string>;
	removeDocument(requestJson:string):Promise<string>;
};
export default rag;

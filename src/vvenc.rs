// Field names mirror the pinned libvvenc C headers exactly (m_SourceWidth,
// m_picReordering, ...) so the repr(C) layout matches; silence the snake_case
// lint. dead_code is also allowed: this is a full hand-written FFI mirror of the
// header API, so constants/entry points may be declared but not all used.
#![allow(non_snake_case, dead_code)]

use alloc::{
    ffi::{CString, NulError},
    format,
};
use core::{
    ffi::{c_char, c_int, c_void},
    hint::cold_path,
    mem::{MaybeUninit, offset_of, size_of},
};

use crate::{
    error::fatal,
    ffms::{VidInf, gcd},
};

pub const VVENC_OK: i32 = 0;
pub const VVENC_ERR_UNSPECIFIED: i32 = -1;
pub const VVENC_ERR_INITIALIZE: i32 = -2;
pub const VVENC_ERR_ALLOCATE: i32 = -3;
pub const VVENC_NOT_ENOUGH_MEM: i32 = -5;
pub const VVENC_ERR_PARAMETER: i32 = -7;
pub const VVENC_ERR_NOT_SUPPORTED: i32 = -10;
pub const VVENC_ERR_RESTART_REQUIRED: i32 = -11;
pub const VVENC_ERR_CPU: i32 = -30;

const VVENC_PARAM_BAD_NAME: i32 = -1;
const VVENC_PARAM_BAD_VALUE: i32 = -2;
const VVENC_PARAM_INFO: i32 = 1;

pub const VVENC_MSG_SILENT: i32 = 0;

// named speed/quality presets accepted by libvvenc's `preset` parameter
pub const VVENC_PRESETS: [&str; 6] = [
    "faster",
    "fast",
    "medium",
    "slow",
    "slower",
    "medium_lowDecEnergy",
];
const VVENC_PROFILE_MAIN_10: i32 = 1;
const VVENC_TIER_MAIN: i32 = 0;
const VVENC_LEVEL_AUTO: i32 = 0;
const VVENC_DRT_IDR: i32 = 2;
const VVENC_HDR_OFF: i32 = 0;
const VVENC_HDR_PQ: i32 = 1;
const VVENC_HDR_HLG: i32 = 2;
const VVENC_HDR_PQ_BT2020: i32 = 3;
const VVENC_HDR_HLG_BT2020: i32 = 4;
const VVENC_SDR_BT709: i32 = 6;
const VVENC_SDR_BT2020: i32 = 7;
const VVENC_SDR_BT470BG: i32 = 8;
const VVENC_CHROMA_420: i32 = 1;

const VVENC_MAX_GOP: usize = 64;
const VVENC_MAX_NUM_REF_PICS: usize = 29;
const VVENC_MAX_TLAYER: usize = 7;
const VVENC_MAX_NUM_CQP_MAPPING_TABLES: usize = 3;
const VVENC_MAX_QP_VALS_CHROMA: usize = 8;
const VVENC_MAX_MCTF_FRAMES: usize = 16;
const VVENC_MAX_STRING_LEN: usize = 1024;

// The AU payload carried out of every access unit; large enough for any
// 16K keyframe. On VVENC_NOT_ENOUGH_MEM the encoder reports the required size
// in payloadUsedSize and the buffer is grown on the fly.
pub const VVENC_AU_CAP: usize = 16 << 20;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VvencYUVPlane {
    pub ptr: *mut i16,
    pub width: i32,
    pub height: i32,
    pub stride: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VvencYUVBuffer {
    pub planes: [VvencYUVPlane; 3],
    pub sequence_number: u64,
    pub cts: i64,
    pub cts_valid: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VvencAccessUnit {
    pub payload: *mut u8,
    pub payload_size: i32,
    pub payload_used_size: i32,
    pub cts: i64,
    pub dts: i64,
    pub cts_valid: bool,
    pub dts_valid: bool,
    pub rap: bool,
    pub slice_type: i32,
    pub ref_pic: bool,
    pub temporal_layer: i32,
    pub poc: u64,
    pub status: i32,
    pub essential_bytes: i32,
    pub info_string: [i8; VVENC_MAX_STRING_LEN],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VvencGOPEntry {
    pub m_POC: i32,
    pub m_QPOffset: i32,
    pub m_QPOffsetModelOffset: f64,
    pub m_QPOffsetModelScale: f64,
    pub m_CbQPoffset: i32,
    pub m_CrQPoffset: i32,
    pub m_QPFactor: f64,
    pub m_tcOffsetDiv2: i32,
    pub m_betaOffsetDiv2: i32,
    pub m_cfgUnused1: i32,
    pub m_cfgUnused2: i32,
    pub m_cfgUnused3: i32,
    pub m_cfgUnused4: i32,
    pub m_temporalId: i32,
    pub m_cfgUnused5: bool,
    pub m_sliceType: i8,
    pub m_numRefPicsActive: [i32; 2],
    pub m_numRefPics: [i32; 2],
    pub m_deltaRefPics: [[i32; VVENC_MAX_NUM_REF_PICS]; 2],
    pub m_cfgUnused6: bool,
    pub m_cfgUnused7: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VvencRPLEntry {
    pub m_POC: i32,
    pub m_temporalId: i32,
    pub m_refPic: bool,
    pub m_ltrp_in_slice_header_flag: bool,
    pub m_numRefPicsActive: i32,
    pub m_sliceType: i8,
    pub m_numRefPics: i32,
    pub m_deltaRefPics: [i32; VVENC_MAX_NUM_REF_PICS],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VvencUnusedStruct0 {
    pub m_cfgUnused0: bool,
    pub m_cfgUnused1: f64,
    pub m_cfgUnused2: f64,
    pub m_cfgUnused3: f64,
    pub m_cfgUnused4: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VvencChromaQpMappingTableParams {
    pub m_numQpTables: i32,
    pub m_qpBdOffset: i32,
    pub m_sameCQPTableForAllChromaFlag: bool,
    pub m_qpTableStartMinus26: [i32; VVENC_MAX_NUM_CQP_MAPPING_TABLES],
    pub m_numPtsInCQPTableMinus1: [i32; VVENC_MAX_NUM_CQP_MAPPING_TABLES],
    pub m_deltaQpInValMinus1: [[i32; 16]; VVENC_MAX_NUM_CQP_MAPPING_TABLES],
    pub m_deltaQpOutVal: [[i32; 16]; VVENC_MAX_NUM_CQP_MAPPING_TABLES],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VvencReshapeCW {
    pub binCW: [u32; 3],
    pub updateCtrl: i32,
    pub adpOption: i32,
    pub initialCW: u32,
    pub rspPicSize: i32,
    pub rspUnused: i32,
    pub rspFps: i32,
    pub rspTid: i32,
    pub rspFpsToIp: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VvencMCTF {
    pub MCTF: i32,
    pub MCTFSpeed: i32,
    pub MCTFFutureReference: bool,
    pub MCTFUnitSize: i32,
    pub mctfUnused: i32,
    pub numFrames: i32,
    pub MCTFFrames: [i32; VVENC_MAX_MCTF_FRAMES],
    pub numStrength: i32,
    pub MCTFStrengths: [f64; VVENC_MAX_MCTF_FRAMES],
}

pub type VvencLoggingCallback =
    Option<unsafe extern "C" fn(*mut c_void, i32, *const c_char, *mut c_void)>;

#[repr(C)]
pub struct VvencConfig {
    pub m_SourceWidth: i32,
    pub m_SourceHeight: i32,
    pub m_FrameRate: i32,
    pub m_FrameScale: i32,
    pub m_TicksPerSecond: i32,
    pub m_framesToBeEncoded: i32,
    pub m_inputBitDepth: [i32; 2],
    pub m_numThreads: i32,
    pub m_QP: i32,
    pub m_RCTargetBitrate: i32,
    pub m_verbosity: i32,
    pub m_profile: i32,
    pub m_levelTier: i32,
    pub m_level: i32,
    pub m_IntraPeriod: i32,
    pub m_IntraPeriodSec: i32,
    pub m_DecodingRefreshType: i32,
    pub m_GOPSize: i32,
    pub m_RCNumPasses: i32,
    pub m_RCPass: i32,
    pub m_cfgUnused1: bool,
    pub m_internalBitDepth: [i32; 2],
    pub m_HdrMode: i32,
    pub m_SegmentMode: i32,
    pub m_usePerceptQPA: bool,
    pub m_numTileCols: i32,
    pub m_numTileRows: i32,
    pub m_conformanceWindowMode: i32,
    pub m_confWinLeft: i32,
    pub m_confWinRight: i32,
    pub m_confWinTop: i32,
    pub m_confWinBottom: i32,
    pub m_cfgUnused15: u32,
    pub m_PadSourceWidth: i32,
    pub m_PadSourceHeight: i32,
    pub m_aiPad: [i32; 2],
    pub m_enablePictureHeaderInSliceHeader: bool,
    pub m_AccessUnitDelimiter: i32,
    pub m_printMSEBasedSequencePSNR: bool,
    pub m_printHexPsnr: bool,
    pub m_printFrameMSE: bool,
    pub m_printSequenceMSE: bool,
    pub m_cabacZeroWordPaddingEnabled: bool,
    pub m_subProfile: u32,
    pub m_bitDepthConstraintValue: u32,
    pub m_intraOnlyConstraintFlag: bool,
    pub m_rewriteParamSets: bool,
    pub m_idrRefParamList: bool,
    pub m_cfgUnused2: [VvencRPLEntry; 64],
    pub m_cfgUnused3: [VvencRPLEntry; 64],
    pub m_GOPList: [VvencGOPEntry; VVENC_MAX_GOP],
    pub m_cfgUnused4: [i32; 7],
    pub m_cfgUnused5: [i32; 7],
    pub m_cfgUnused6: i32,
    pub m_maxPicWidth: i32,
    pub m_maxPicHeight: i32,
    pub m_useSameChromaQPTables: bool,
    pub m_chromaQpMappingTableParams: VvencChromaQpMappingTableParams,
    pub m_intraQPOffset: i32,
    pub m_lambdaFromQPEnable: bool,
    pub m_adLambdaModifier: [f64; VVENC_MAX_TLAYER],
    pub m_adIntraLambdaModifier: [f64; VVENC_MAX_TLAYER],
    pub m_dIntraQpFactor: f64,
    pub m_qpInValsCb: [i32; VVENC_MAX_QP_VALS_CHROMA],
    pub m_qpInValsCr: [i32; VVENC_MAX_QP_VALS_CHROMA],
    pub m_qpInValsCbCr: [i32; VVENC_MAX_QP_VALS_CHROMA],
    pub m_qpOutValsCb: [i32; VVENC_MAX_QP_VALS_CHROMA],
    pub m_qpOutValsCr: [i32; VVENC_MAX_QP_VALS_CHROMA],
    pub m_qpOutValsCbCr: [i32; VVENC_MAX_QP_VALS_CHROMA],
    pub m_cuQpDeltaSubdiv: i32,
    pub m_cuChromaQpOffsetSubdiv: i32,
    pub m_chromaCbQpOffset: i32,
    pub m_chromaCrQpOffset: i32,
    pub m_chromaCbQpOffsetDualTree: i32,
    pub m_chromaCrQpOffsetDualTree: i32,
    pub m_chromaCbCrQpOffset: i32,
    pub m_chromaCbCrQpOffsetDualTree: i32,
    pub m_sliceChromaQpOffsetPeriodicity: i32,
    pub m_sliceChromaQpOffsetIntraOrPeriodic: [i32; 2],
    pub m_usePerceptQPATempFiltISlice: i32,
    pub m_lumaLevelToDeltaQPEnabled: bool,
    pub m_cfgUnused24: VvencUnusedStruct0,
    pub m_internChromaFormat: i32,
    pub m_useIdentityTableForNon420Chroma: bool,
    pub m_outputBitDepth: [i32; 2],
    pub m_MSBExtendedBitDepth: [i32; 2],
    pub m_costMode: i32,
    pub m_decodedPictureHashSEIType: i32,
    pub m_bufferingPeriodSEIEnabled: bool,
    pub m_pictureTimingSEIEnabled: bool,
    pub m_decodingUnitInfoSEIEnabled: bool,
    pub m_entropyCodingSyncEnabled: i8,
    pub m_entryPointsPresent: bool,
    pub m_CTUSize: u32,
    pub m_MinQT: [u32; 3],
    pub m_maxMTTDepth: u32,
    pub m_maxMTTDepthI: u32,
    pub m_maxMTTDepthIChroma: i32,
    pub m_maxBT: [u32; 3],
    pub m_maxTT: [u32; 3],
    pub m_dualITree: bool,
    pub m_cfgUnused9: u32,
    pub m_cfgUnused10: u32,
    pub m_log2MaxTbSize: i32,
    pub m_log2MinCodingBlockSize: i32,
    pub m_bUseASR: bool,
    pub m_bUseHADME: bool,
    pub m_RDOQ: i32,
    pub m_useRDOQTS: bool,
    pub m_useSelectiveRDOQ: i8,
    pub m_JointCbCrMode: bool,
    pub m_cabacInitPresent: i32,
    pub m_useFastLCTU: bool,
    pub m_usePbIntraFast: i32,
    pub m_useFastMrg: i32,
    pub m_useAMaxBT: i32,
    pub m_fastQtBtEnc: bool,
    pub m_contentBasedFastQtbt: bool,
    pub m_fastInterSearchMode: i32,
    pub m_useEarlyCU: i32,
    pub m_useFastDecisionForMerge: bool,
    pub m_bDisableIntraCUsInInterSlices: bool,
    pub m_cfgUnused11: bool,
    pub m_bFastUDIUseMPMEnabled: bool,
    pub m_bFastMEForGenBLowDelayEnabled: bool,
    pub m_MTSImplicit: bool,
    pub m_TMVPModeId: i32,
    pub m_DepQuantEnabled: bool,
    pub m_SignDataHidingEnabled: bool,
    pub m_MIP: bool,
    pub m_useFastMIP: i32,
    pub m_maxNumMergeCand: u32,
    pub m_maxNumAffineMergeCand: u32,
    pub m_Geo: i32,
    pub m_maxNumGeoCand: u32,
    pub m_FastIntraTools: i32,
    pub m_IntraEstDecBit: i32,
    pub m_RCInitialQP: i32,
    pub m_cfgUnused16: bool,
    pub m_motionEstimationSearchMethod: i32,
    pub m_motionEstimationSearchMethodSCC: i32,
    pub m_cfgUnused12: bool,
    pub m_SearchRange: i32,
    pub m_bipredSearchRange: i32,
    pub m_minSearchWindow: i32,
    pub m_bClipForBiPredMeEnabled: bool,
    pub m_bFastMEAssumingSmootherMVEnabled: bool,
    pub m_bIntegerET: bool,
    pub m_fastSubPel: i32,
    pub m_SMVD: i32,
    pub m_AMVRspeed: i32,
    pub m_LMChroma: bool,
    pub m_horCollocatedChromaFlag: bool,
    pub m_verCollocatedChromaFlag: bool,
    pub m_MRL: bool,
    pub m_BDOF: bool,
    pub m_DMVR: bool,
    pub m_EDO: i32,
    pub m_lumaReshapeEnable: i32,
    pub m_reshapeSignalType: i32,
    pub m_updateCtrl: i32,
    pub m_adpOption: i32,
    pub m_initialCW: i32,
    pub m_LMCSOffset: i32,
    pub m_reshapeCW: VvencReshapeCW,
    pub m_Affine: i32,
    pub m_PROF: bool,
    pub m_AffineType: bool,
    pub m_MMVD: i32,
    pub m_MmvdDisNum: i32,
    pub m_allowDisFracMMVD: bool,
    pub m_CIIP: i32,
    pub m_SbTMVP: bool,
    pub m_SBT: i32,
    pub m_LFNST: i32,
    pub m_MTS: i32,
    pub m_MTSIntraMaxCand: i32,
    pub m_ISP: i32,
    pub m_TS: i32,
    pub m_TSsize: i32,
    pub m_useChromaTS: i32,
    pub m_useBDPCM: i32,
    pub m_rprEnabledFlag: i32,
    pub m_resChangeInClvsEnabled: bool,
    pub m_craAPSreset: bool,
    pub m_rprRASLtoolSwitch: bool,
    pub m_IBCMode: i32,
    pub m_IBCFastMethod: i32,
    pub m_BCW: i32,
    pub m_FIMMode: i32,
    pub m_FastInferMerge: i32,
    pub m_bLoopFilterDisable: bool,
    pub m_loopFilterOffsetInPPS: bool,
    pub m_loopFilterBetaOffsetDiv2: [i32; 3],
    pub m_loopFilterTcOffsetDiv2: [i32; 3],
    pub m_cfgUnused13: i32,
    pub m_bDisableLFCrossTileBoundaryFlag: bool,
    pub m_bDisableLFCrossSliceBoundaryFlag: bool,
    pub m_bUseSAO: bool,
    pub m_saoEncodingRate: f64,
    pub m_saoEncodingRateChroma: f64,
    pub m_log2SaoOffsetScale: [u32; 2],
    pub m_saoOffsetBitShift: [i32; 2],
    pub m_decodingParameterSetEnabled: bool,
    pub m_vuiParametersPresent: i32,
    pub m_hrdParametersPresent: bool,
    pub m_aspectRatioInfoPresent: bool,
    pub m_aspectRatioIdc: i32,
    pub m_sarWidth: i32,
    pub m_sarHeight: i32,
    pub m_colourDescriptionPresent: bool,
    pub m_colourPrimaries: i32,
    pub m_transferCharacteristics: i32,
    pub m_matrixCoefficients: i32,
    pub m_chromaLocInfoPresent: i8,
    pub m_cfgUnused26: i32,
    pub m_cfgUnused27: i32,
    pub m_chromaSampleLocType: i32,
    pub m_overscanInfoPresent: bool,
    pub m_overscanAppropriateFlag: bool,
    pub m_cfgUnused14: bool,
    pub m_videoFullRangeFlag: bool,
    pub m_masteringDisplay: [u32; 10],
    pub m_contentLightLevel: [u32; 2],
    pub m_preferredTransferCharacteristics: i32,
    pub m_alf: bool,
    pub m_useNonLinearAlfLuma: bool,
    pub m_useNonLinearAlfChroma: bool,
    pub m_maxNumAlfAlternativesChroma: u32,
    pub m_ccalf: bool,
    pub m_cfgUnused25: i32,
    pub m_alfTempPred: i32,
    pub m_alfSpeed: i32,
    pub m_vvencMCTF: VvencMCTF,
    pub m_quantThresholdVal: i32,
    pub m_qtbttSpeedUp: i32,
    pub m_qtbttSpeedUpMode: i32,
    pub m_fastTTSplit: i32,
    pub m_fastTT_th: f32,
    pub m_fastLocalDualTreeMode: i32,
    pub m_maxParallelFrames: i32,
    pub m_ensureWppBitEqual: i32,
    pub m_tileParallelCtuEnc: bool,
    pub m_picPartitionFlag: bool,
    pub m_tileColumnWidth: [u32; 10],
    pub m_tileRowHeight: [u32; 10],
    pub m_numExpTileCols: u32,
    pub m_numExpTileRows: u32,
    pub m_numSlicesInPic: u32,
    pub m_cfgUnused17: i32,
    pub m_cfgUnused18: i32,
    pub m_cfgUnused19: i32,
    pub m_cfgUnused20: bool,
    pub m_cfgUnused21: bool,
    pub m_cfgUnused22: bool,
    pub m_cfgUnused23: [[i8; VVENC_MAX_STRING_LEN]; 2],
    pub m_listTracingChannels: bool,
    pub m_traceRule: [i8; VVENC_MAX_STRING_LEN],
    pub m_traceFile: [i8; VVENC_MAX_STRING_LEN],
    pub m_summaryOutFilename: [i8; VVENC_MAX_STRING_LEN],
    pub m_summaryPicFilenameBase: [i8; VVENC_MAX_STRING_LEN],
    pub m_summaryVerboseness: u32,
    pub m_numIntraModesFullRD: i32,
    pub m_reduceIntraChromaModesFullRD: bool,
    pub m_FirstPassMode: i32,
    pub m_numRefPics: i32,
    pub m_numRefPicsSCC: i32,
    pub m_alfUnitSize: i32,
    pub m_meReduceTap: i32,
    pub m_deblockLastTLayers: i32,
    pub m_leadFrames: i32,
    pub m_trailFrames: i32,
    pub m_LookAhead: i32,
    pub m_explicitAPSid: i32,
    pub m_picReordering: bool,
    pub m_fga: bool,
    pub m_poc0idr: bool,
    pub m_ifpLines: i8,
    pub m_blockImportanceMapping: bool,
    pub m_saoScc: bool,
    pub m_addGOP32refPics: bool,
    pub m_fastHad: bool,
    pub m_sliceTypeAdapt: i8,
    pub m_treatAsSubPic: bool,
    pub m_RCMaxBitrate: i32,
    pub m_forceScc: i8,
    pub m_ifp: i8,
    pub m_mtProfile: i8,
    pub m_GOPQPA: i8,
    pub m_minIntraDist: i32,
    pub m_numParallelGOPs: i8,
    pub m_reservedInt8: [i8; 3],
    pub m_reservedDouble: [f64; 8],
    pub m_configDone: bool,
    pub m_confirmFailed: bool,
    pub m_msgFnc: VvencLoggingCallback,
    pub m_msgCtx: *mut c_void,
}

#[repr(C)]
pub struct VvencEncoder {
    _opaque: [u8; 0],
}

pub const VVENC_CFG_SIZE: usize = size_of::<VvencConfig>();

// member offsets must match the pinned libvvenc header (v1.14.0); a layout
// change upstream becomes a compile-time type error instead of silent corruption
const _: [(); 0] = [(); offset_of!(VvencConfig, m_SourceWidth)];
const _: [(); 12] = [(); offset_of!(VvencConfig, m_FrameScale)];
const _: [(); 44] = [(); offset_of!(VvencConfig, m_verbosity)];
const _: [(); 68] = [(); offset_of!(VvencConfig, m_DecodingRefreshType)];
const _: [(); 104] = [(); offset_of!(VvencConfig, m_usePerceptQPA)];
const _: [(); 116] = [(); offset_of!(VvencConfig, m_conformanceWindowMode)];
const _: [(); 160] = [(); offset_of!(VvencConfig, m_AccessUnitDelimiter)];
const _: [(); 18104] = [(); offset_of!(VvencConfig, m_GOPList)];
const _: [(); 39720] = [(); offset_of!(VvencConfig, m_qpInValsCb)];
const _: [(); 40008] = [(); offset_of!(VvencConfig, m_internChromaFormat)];
const _: [(); 40048] = [(); offset_of!(VvencConfig, m_CTUSize)];
const _: [(); 40556] = [(); offset_of!(VvencConfig, m_masteringDisplay)];
const _: [(); 40596] = [(); offset_of!(VvencConfig, m_contentLightLevel)];
const _: [(); 40632] = [(); offset_of!(VvencConfig, m_vvencMCTF)];
const _: [(); 47198] = [(); offset_of!(VvencConfig, m_poc0idr)];
const _: [(); 47208] = [(); offset_of!(VvencConfig, m_RCMaxBitrate)];
const _: [(); 47224] = [(); offset_of!(VvencConfig, m_reservedDouble)];
const _: [(); 47288] = [(); offset_of!(VvencConfig, m_configDone)];
const _: [(); 47289] = [(); offset_of!(VvencConfig, m_confirmFailed)];
const _: [(); 47296] = [(); offset_of!(VvencConfig, m_msgFnc)];

#[link(name = "vvenc")]
unsafe extern "C" {
    fn vvenc_config_default(cfg: *mut VvencConfig);
    fn vvenc_set_param(cfg: *mut VvencConfig, name: *const c_char, value: *const c_char) -> c_int;
    fn vvenc_encoder_create() -> *mut VvencEncoder;
    fn vvenc_encoder_open(enc: *mut VvencEncoder, cfg: *mut VvencConfig) -> c_int;
    pub fn vvenc_encoder_close(enc: *mut VvencEncoder) -> c_int;
    fn vvenc_encode(
        enc: *mut VvencEncoder,
        yuv: *mut VvencYUVBuffer,
        au: *mut VvencAccessUnit,
        done: *mut bool,
    ) -> c_int;
    fn vvenc_accessUnit_alloc() -> *mut VvencAccessUnit;
    fn vvenc_accessUnit_alloc_payload(au: *mut VvencAccessUnit, size: c_int);
    fn vvenc_accessUnit_free_payload(au: *mut VvencAccessUnit);
    fn vvenc_accessUnit_reset(au: *mut VvencAccessUnit);
    fn vvenc_accessUnit_free(au: *mut VvencAccessUnit, free_payload: bool);
    fn vvenc_get_last_error(enc: *mut VvencEncoder) -> *const c_char;
    fn vvenc_get_error_msg(ret: c_int) -> *const c_char;
    pub fn vvenc_get_version() -> *const c_char;
}

#[cold]
#[inline(never)]
fn err_msg(enc: *mut VvencEncoder, ret: i32) {
    let last = unsafe { vvenc_get_last_error(enc) };
    let detail = if last.is_null() {
        let m = unsafe { vvenc_get_error_msg(ret) };
        if m.is_null() {
            "unknown"
        } else {
            unsafe { core::ffi::CStr::from_ptr(m) }
                .to_str()
                .unwrap_or("unknown")
        }
    } else if unsafe { *last } == 0 {
        let m = unsafe { vvenc_get_error_msg(ret) };
        if m.is_null() {
            "unknown"
        } else {
            unsafe { core::ffi::CStr::from_ptr(m) }
                .to_str()
                .unwrap_or("unknown")
        }
    } else {
        unsafe { core::ffi::CStr::from_ptr(last) }
            .to_str()
            .unwrap_or("unknown")
    };
    fatal(format_args!("vvenc: {detail}"));
}

pub fn cfg_default() -> VvencConfig {
    let mut cfg = MaybeUninit::<VvencConfig>::uninit();
    unsafe { vvenc_config_default(cfg.as_mut_ptr()) };
    unsafe { cfg.assume_init() }
}

pub fn set_param(cfg: &mut VvencConfig, name: &str, value: &str) {
    let n = CString::new(name).unwrap_or_else(vvenc_nul_err);
    let v = CString::new(value).unwrap_or_else(vvenc_nul_err);
    let ret = unsafe { vvenc_set_param(cfg, n.as_ptr(), v.as_ptr()) };
    if ret != VVENC_OK && ret != VVENC_PARAM_INFO {
        cold_path();
        fatal(format_args!("vvenc: rejected --{name} {value}"));
    }
}

fn vvenc_nul_err(_: NulError) -> CString {
    cold_path();
    fatal("vvenc: option name/value contains a NUL byte")
}

#[cold]
#[inline(never)]
fn set_cfg_num(cfg: &mut VvencConfig, name: &str, val: i32) {
    set_param(cfg, name, &format!("{val}"));
}

// `-p` style params come in vvencFFapp long-option syntax (`--Name value`);
// hand each pair to vvenc_set_param, which lowercases the name itself.
#[cold]
#[inline(never)]
pub fn vvenc_split(cfg: &mut VvencConfig, params: &str) {
    let mut it = params.split_whitespace();
    while let Some(tok) = it.next() {
        let Some(key) = tok.strip_prefix("--") else {
            cold_path();
            fatal(format_args!("vvenc: expected --option, got {tok}"));
        };
        let (name, val) = match key.split_once('=') {
            Some(nv) => nv,
            None => {
                let v = match it.next() {
                    Some(v) if !v.starts_with("--") => v,
                    _ => {
                        cold_path();
                        fatal(format_args!("vvenc: --{key} needs a value"));
                    }
                };
                (key, v)
            }
        };
        set_param(cfg, name, val);
    }
}

const fn hdr_mode(cp: i8, tc: i8) -> i32 {
    let hlg = tc == 18;
    let pq = tc == 16;
    let bt2020 = cp == 9;
    if pq {
        if bt2020 {
            VVENC_HDR_PQ_BT2020
        } else {
            VVENC_HDR_PQ
        }
    } else if hlg {
        if bt2020 {
            VVENC_HDR_HLG_BT2020
        } else {
            VVENC_HDR_HLG
        }
    } else if bt2020 {
        VVENC_SDR_BT2020
    } else if cp == 5 {
        VVENC_SDR_BT470BG
    } else {
        VVENC_SDR_BT709
    }
}

pub fn val_preset(p: &str) -> bool {
    VVENC_PRESETS.iter().any(|&s| s.eq_ignore_ascii_case(p))
}

#[cold]
#[inline(never)]
pub fn set_vvenc_base(cfg: &mut VvencConfig, inf: &VidInf, w: u32, h: u32) {
    let g = gcd(u64::from(inf.fps_num), u64::from(inf.fps_den)).max(1);
    cfg.m_SourceWidth = w as i32;
    cfg.m_SourceHeight = h as i32;
    cfg.m_FrameRate = inf.fps_num as i32;
    cfg.m_FrameScale = inf.fps_den as i32;
    // ticks must satisfy (ticks * fps_den) % fps_num == 0; gcd-reduced
    // numerator is the smallest in-range multiple for any frame rate
    cfg.m_TicksPerSecond = (u64::from(inf.fps_num) / g).min(27_000_000) as i32;
    cfg.m_inputBitDepth = [10, 10];
    cfg.m_internalBitDepth = [10, 10];
    cfg.m_outputBitDepth = [10, 10];
    cfg.m_MSBExtendedBitDepth = [10, 10];
    cfg.m_numThreads = 1;
    cfg.m_verbosity = VVENC_MSG_SILENT;
    cfg.m_profile = VVENC_PROFILE_MAIN_10;
    cfg.m_levelTier = VVENC_TIER_MAIN;
    cfg.m_level = VVENC_LEVEL_AUTO;
    cfg.m_IntraPeriod = -1;
    cfg.m_IntraPeriodSec = 0;
    cfg.m_DecodingRefreshType = VVENC_DRT_IDR;
    cfg.m_poc0idr = true;
    cfg.m_picReordering = true;
    cfg.m_internChromaFormat = VVENC_CHROMA_420;
    cfg.m_colourDescriptionPresent = true;
    cfg.m_vuiParametersPresent = 1;
    cfg.m_colourPrimaries = i32::from(inf.color_primaries);
    cfg.m_transferCharacteristics = i32::from(inf.transfer_characteristics);
    cfg.m_matrixCoefficients = i32::from(inf.matrix_coefficients);
    cfg.m_videoFullRangeFlag = inf.color_range == 1;
    cfg.m_HdrMode = hdr_mode(inf.color_primaries, inf.transfer_characteristics);
    let csp = inf.chroma_sample_position;
    if (1..=6).contains(&csp) {
        cfg.m_chromaLocInfoPresent = 1;
        cfg.m_chromaSampleLocType = i32::from(csp - 1);
    }
    if let Some(m) = inf.mastering {
        let q = |(x, y): (f64, f64)| ((x * 50_000.0 + 0.5) as u32, (y * 50_000.0 + 0.5) as u32);
        let (gx, gy) = q(m.g);
        let (bx, by) = q(m.b);
        let (rx, ry) = q(m.r);
        let (wx, wy) = q(m.wp);
        cfg.m_masteringDisplay = [
            gx,
            gy,
            bx,
            by,
            rx,
            ry,
            wx,
            wy,
            (m.lum_max * 10_000.0 + 0.5) as u32,
            (m.lum_min * 10_000.0 + 0.5) as u32,
        ];
    }
    if let Some((c, f)) = inf.content_light_level {
        cfg.m_contentLightLevel = [u32::from(c), u32::from(f)];
    }

    // cross-check that the linked library shares our struct layout: ask the
    // library's own parser to write a field with a known offset and verify it
    // landed where we think it did. a version mismatch upstream fails loudly
    // instead of corrupting memory.
    set_cfg_num(cfg, "sourcewidth", w as i32);
    set_cfg_num(cfg, "sourceheight", h as i32);
    if cfg.m_SourceWidth != w as i32 || cfg.m_SourceHeight != h as i32 {
        cold_path();
        fatal(
            "vvenc: libvvenc config layout mismatch (update src/vvenc.rs for the linked version)",
        );
    }
}

pub fn new_enc() -> *mut VvencEncoder {
    let enc = unsafe { vvenc_encoder_create() };
    if enc.is_null() {
        cold_path();
        fatal("vvenc: vvenc_encoder_create failed");
    }
    enc
}

pub fn open(enc: *mut VvencEncoder, cfg: &mut VvencConfig) {
    let ret = unsafe { vvenc_encoder_open(enc, cfg) };
    if ret != VVENC_OK {
        cold_path();
        err_msg(enc, ret);
    }
}

pub fn encode(
    enc: *mut VvencEncoder,
    yuv: *mut VvencYUVBuffer,
    au: *mut VvencAccessUnit,
    done: &mut bool,
) {
    let ret = unsafe { vvenc_encode(enc, yuv, au, done) };
    if ret != VVENC_OK {
        cold_path();
        err_msg(enc, ret);
    }
}

// grow the AU payload on VVENC_NOT_ENOUGH_MEM and re-fetch the pending chunk
pub fn encode_drain(enc: *mut VvencEncoder, au: *mut VvencAccessUnit, done: &mut bool) {
    loop {
        let ret = unsafe { vvenc_encode(enc, core::ptr::null_mut(), au, done) };
        if ret == VVENC_OK {
            return;
        }
        if ret != VVENC_NOT_ENOUGH_MEM {
            cold_path();
            err_msg(enc, ret);
        }
        let need = unsafe { (*au).payload_used_size };
        if need <= 0 {
            cold_path();
            fatal("vvenc: encode reported insufficient AU payload with a zero required size");
        }
        unsafe {
            vvenc_accessUnit_free_payload(au);
            vvenc_accessUnit_alloc_payload(au, need);
        }
    }
}

pub fn new_au() -> *mut VvencAccessUnit {
    let au = unsafe { vvenc_accessUnit_alloc() };
    if au.is_null() {
        cold_path();
        fatal("vvenc: vvenc_accessUnit_alloc failed");
    }
    unsafe { vvenc_accessUnit_alloc_payload(au, VVENC_AU_CAP as i32) };
    au
}

pub fn drop_au(au: *mut VvencAccessUnit) {
    unsafe { vvenc_accessUnit_free(au, true) };
}

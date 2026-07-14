import type { CrimeId, CrimeName, SentencingData } from "./types.ts";

/**
 * 量刑计算器 — 量刑规则数据
 * 依据：最高法量刑指导意见（法发2021-21号）
 *       广东省量刑实施细则（粤高法发2017-6号）
 *       法发〔2016〕32号、法释〔2026〕6号等专项文件
 * 最后复核：2026-07-14。具体适用仍须结合行为时间、案件事实和现行文件人工判断。
 */

export const SENTENCING_DATA: SentencingData = {
  // ==================== 罪名列表 ====================
  crimes: [
    { id: 'theft', name: '盗窃罪', desc: '以非法占有为目的，盗窃公私财物数额较大的行为' },
    { id: 'fraud', name: '诈骗罪', desc: '以非法占有为目的，诈骗公私财物数额较大的行为' },
    { id: 'contract_fraud', name: '合同诈骗罪', desc: '以非法占有为目的，在签订、履行合同过程中骗取对方财物' },
    { id: 'embezzlement', name: '职务侵占罪', desc: '公司、企业或其他单位人员利用职务便利非法占有本单位财物' },
    { id: 'disturbance', name: '寻衅滋事罪', desc: '随意殴打、追逐拦截、强拿硬要、任意损毁、起哄闹事等行为' },
    { id: 'dangerous_driving', name: '危险驾驶罪', desc: '在道路上驾驶机动车追逐竞驶、醉酒驾驶等危险驾驶行为' },
    { id: 'traffic_accident', name: '交通肇事罪', desc: '违反交通运输管理法规导致重大事故的行为' },
    { id: 'non_official_bribery', name: '非国家工作人员受贿罪', desc: '公司、企业或其他单位人员利用职务便利索取或非法收受他人财物' },
    { id: 'corruption', name: '贪污罪', desc: '国家工作人员利用职务便利侵吞、窃取、骗取公共财物' },
    { id: 'bribery', name: '受贿罪', desc: '国家工作人员利用职务便利索取或非法收受他人财物' },
    { id: 'intentional_injury', name: '故意伤害罪', desc: '故意非法损害他人身体健康的行为' },
    { id: 'robbery', name: '抢劫罪', desc: '以非法占有为目的，以暴力、胁迫或其他方法强行劫取财物的行为' },
    { id: 'aiding_cyber_crime', name: '帮助信息网络犯罪活动罪', desc: '明知他人利用信息网络实施犯罪，为其提供技术支持、广告推广、支付结算等帮助' },
  ],

  // ==================== 量刑标准 ====================
  // 每个罪名的各档位起刑点
  standards: {
    theft: [
      // 一类地区
      { area: '一类地区', tier: '较大',   minAmount: 3000,   maxAmount: 100000,   startMin: 0,  startMax: 12 },
      { area: '一类地区', tier: '巨大',   minAmount: 100000, maxAmount: 500000,   startMin: 36, startMax: 48 },
      { area: '一类地区', tier: '特别巨大', minAmount: 500000, maxAmount: null,    startMin: 120, startMax: 144 },
      // 二类地区
      { area: '二类地区', tier: '较大',   minAmount: 2000,   maxAmount: 100000,   startMin: 0,  startMax: 12 },
      { area: '二类地区', tier: '巨大',   minAmount: 100000, maxAmount: 500000,   startMin: 36, startMax: 48 },
      { area: '二类地区', tier: '特别巨大', minAmount: 500000, maxAmount: null,    startMin: 120, startMax: 144 },
    ],
    fraud: [
      { area: '一类地区', tier: '较大',   minAmount: 6000,   maxAmount: 100000, startMin: 0,  startMax: 12 },
      { area: '一类地区', tier: '巨大',   minAmount: 100000, maxAmount: 500000, startMin: 36, startMax: 48 },
      { area: '一类地区', tier: '特别巨大', minAmount: 500000, maxAmount: null,  startMin: 120, startMax: 144 },
      { area: '二类地区', tier: '较大',   minAmount: 4000,   maxAmount: 100000, startMin: 0,  startMax: 12 },
      { area: '二类地区', tier: '巨大',   minAmount: 100000, maxAmount: 500000, startMin: 36, startMax: 48 },
      { area: '二类地区', tier: '特别巨大', minAmount: 500000, maxAmount: null,  startMin: 120, startMax: 144 },
      // 电信网络诈骗独立标准（法发〔2016〕32号）
      { area: '全国', tier: '较大',   minAmount: 3000,   maxAmount: 30000,  startMin: 0,  startMax: 36, subType: '电信诈骗' },
      { area: '全国', tier: '巨大',   minAmount: 30000,  maxAmount: 500000, startMin: 36, startMax: 120, subType: '电信诈骗' },
      { area: '全国', tier: '特别巨大', minAmount: 500000, maxAmount: null,   startMin: 120, startMax: 144, subType: '电信诈骗' },
    ],
    contract_fraud: [
      { area: '一类地区', tier: '较大',   minAmount: 20000,  maxAmount: 300000,  startMin: 0,  startMax: 12 },
      { area: '一类地区', tier: '巨大',   minAmount: 300000, maxAmount: 1500000, startMin: 36, startMax: 48 },
      { area: '一类地区', tier: '特别巨大', minAmount: 1500000, maxAmount: null, startMin: 120, startMax: 144 },
      { area: '二类地区', tier: '较大',   minAmount: 20000,  maxAmount: 200000,  startMin: 0,  startMax: 12 },
      { area: '二类地区', tier: '巨大',   minAmount: 200000, maxAmount: 1200000, startMin: 36, startMax: 48 },
      { area: '二类地区', tier: '特别巨大', minAmount: 1200000, maxAmount: null, startMin: 120, startMax: 144 },
      { area: '全国',    tier: '较大',   minAmount: 20000,  maxAmount: 200000,  startMin: 0,  startMax: 12 },
      { area: '全国',    tier: '巨大',   minAmount: 200000, maxAmount: 1000000, startMin: 36, startMax: 48 },
      { area: '全国',    tier: '特别巨大', minAmount: 1000000, maxAmount: null, startMin: 120, startMax: 144 },
    ],
    embezzlement: [
      // 2026.5.1前旧标准
      { area: '全国', tier: '较大',   minAmount: 60000,   maxAmount: 1000000,  startMin: 0,  startMax: 24, effTo: '2026-04-30' },
      { area: '全国', tier: '巨大',   minAmount: 1000000, maxAmount: 15000000, startMin: 60, startMax: 72, effTo: '2026-04-30' },
      { area: '全国', tier: '特别巨大', minAmount: 15000000, maxAmount: null,   startMin: 120, startMax: 132, effTo: '2026-04-30' },
      // 2026.5.1起新标准
      { area: '全国', tier: '较大',   minAmount: 30000,   maxAmount: 200000,   startMin: 0,  startMax: 12, effFrom: '2026-05-01' },
      { area: '全国', tier: '巨大',   minAmount: 200000,  maxAmount: 3000000,  startMin: 36, startMax: 48, effFrom: '2026-05-01' },
      { area: '全国', tier: '特别巨大', minAmount: 3000000, maxAmount: null,    startMin: 120, startMax: 132, effFrom: '2026-05-01' },
    ],
    disturbance: [
      { area: '全国', tier: '一次', minAmount: null, maxAmount: null, startMin: 0,  startMax: 36 },
      { area: '全国', tier: '三次', minAmount: null, maxAmount: null, startMin: 60, startMax: 84 },
    ],
    dangerous_driving: [
      { area: '全国', tier: '标准', minAmount: null, maxAmount: null, startMin: 1, startMax: 6 },
    ],
    traffic_accident: [
      { area: '全国', tier: '基本',     minAmount: null, maxAmount: null, startMin: 0,  startMax: 24 },
      { area: '全国', tier: '逃逸',     minAmount: null, maxAmount: null, startMin: 36, startMax: 60 },
      { area: '全国', tier: '逃逸致死', minAmount: null, maxAmount: null, startMin: 84, startMax: 120 },
    ],
    non_official_bribery: [
      { area: '全国', tier: '较大',   minAmount: 60000,  maxAmount: 1000000, startMin: 0,  startMax: 24, effTo: '2026-04-30' },
      { area: '全国', tier: '巨大',   minAmount: 1000000, maxAmount: null,   startMin: 60, startMax: 72, effTo: '2026-04-30' },
      { area: '全国', tier: '较大',   minAmount: 30000,  maxAmount: 200000,  startMin: 0,  startMax: 24, effFrom: '2026-05-01' },
      { area: '全国', tier: '巨大',   minAmount: 200000, maxAmount: 3000000, startMin: 60, startMax: 72, effFrom: '2026-05-01' },
      { area: '全国', tier: '特别巨大', minAmount: 3000000, maxAmount: null,  startMin: 120, startMax: 132, effFrom: '2026-05-01' },
    ],
    corruption: [
      { area: '全国', tier: '较大',   minAmount: 30000,  maxAmount: 200000,  startMin: 0,  startMax: 36 },
      { area: '全国', tier: '巨大',   minAmount: 200000, maxAmount: 3000000, startMin: 36, startMax: 120 },
      { area: '全国', tier: '特别巨大', minAmount: 3000000, maxAmount: null,  startMin: 120, startMax: null },
    ],
    bribery: [
      { area: '全国', tier: '较大',   minAmount: 30000,  maxAmount: 200000,  startMin: 0,  startMax: 36 },
      { area: '全国', tier: '巨大',   minAmount: 200000, maxAmount: 3000000, startMin: 36, startMax: 120 },
      { area: '全国', tier: '特别巨大', minAmount: 3000000, maxAmount: null,  startMin: 120, startMax: null },
    ],
    // ===== 新增罪名 =====
    intentional_injury: [
      { area: '全国', tier: '轻伤',   minAmount: null, maxAmount: null, startMin: 0,  startMax: 36 },
      { area: '全国', tier: '重伤',   minAmount: null, maxAmount: null, startMin: 36, startMax: 120 },
      { area: '全国', tier: '致死/严重残疾', minAmount: null, maxAmount: null, startMin: 120, startMax: null },
    ],
    robbery: [
      { area: '全国', tier: '基本',     minAmount: null, maxAmount: null, startMin: 36, startMax: 120 },
      { area: '全国', tier: '加重',     minAmount: null, maxAmount: null, startMin: 120, startMax: null },
    ],
    aiding_cyber_crime: [
      { area: '全国', tier: '情节严重', minAmount: null, maxAmount: null, startMin: 0, startMax: 36 },
    ],
  },

  // ==================== 增加刑罚量规则 ====================
  increments: {
    theft: [
      { area: '一类地区', tier: '较大',   perAmount: 15000, penaltyMin: 3, penaltyMax: 6 },
      { area: '一类地区', tier: '巨大',   perAmount: 50000, penaltyMin: 6, penaltyMax: 12 },
      { area: '一类地区', tier: '特别巨大', perAmount: 0, penaltyMin: 0,  penaltyMax: 12,   maxCap: 500000 },
      { area: '一类地区', tier: '特别巨大', perAmount: 0, penaltyMin: 12, penaltyMax: 36,  maxCap: 2500000 },
      { area: '一类地区', tier: '特别巨大', perAmount: 0, penaltyMin: 36, penaltyMax: null },
    ],
    fraud: [
      { area: '一类地区', tier: '较大',   perAmount: 15000, penaltyMin: 3,  penaltyMax: 6,  subType: '一般诈骗' },
      { area: '一类地区', tier: '巨大',   perAmount: 40000, penaltyMin: 6,  penaltyMax: 12, subType: '一般诈骗' },
      { area: '全国', tier: '较大',   perAmount: 5000,  penaltyMin: 3,  penaltyMax: 6,  subType: '电信诈骗' },
      { area: '全国', tier: '巨大',   perAmount: 50000, penaltyMin: 6,  penaltyMax: 12, subType: '电信诈骗' },
      { area: '一类地区', tier: '特别巨大', perAmount: 0, penaltyMin: 0,  penaltyMax: 12,  maxCap: 500000 },
      { area: '一类地区', tier: '特别巨大', perAmount: 0, penaltyMin: 12, penaltyMax: 36,  maxCap: 2500000 },
      { area: '一类地区', tier: '特别巨大', perAmount: 0, penaltyMin: 36, penaltyMax: null },
    ],
    embezzlement: [
      { area: '全国', tier: '较大', perAmount: 50000, penaltyMin: 1, penaltyMax: 3 },
      { area: '全国', tier: '巨大', perAmount: 0, penaltyMin: 0,  penaltyMax: 36,  maxCap: 5000000 },
      { area: '全国', tier: '巨大', perAmount: 0, penaltyMin: 36, penaltyMax: 60,  maxCap: 10000000 },
      { area: '全国', tier: '巨大', perAmount: 0, penaltyMin: 60, penaltyMax: null },
    ],
    // 其他罪名暂无细化增量规则
  },

  // ==================== 量刑情节 ====================
  // 优先情节（先调节，连乘）
  priorityFactors: [
    { id: 'minor_12_16',     name: '未成年人12-16岁',     direction: 'reduce', minPct: -60, maxPct: -30 },
    { id: 'minor_16_18',     name: '未成年人16-18岁',     direction: 'reduce', minPct: -50, maxPct: -10 },
    { id: 'elderly_intent',  name: '老年人≥75岁故意',     direction: 'reduce', minPct: -40, maxPct: 0 },
    { id: 'elderly_negligent', name: '老年人≥75岁过失',   direction: 'reduce', minPct: -50, maxPct: -20 },
    { id: 'deaf_mute',       name: '又聋又哑的人',        direction: 'reduce', minPct: -50, maxPct: 0 },
    { id: 'blind',           name: '盲人',                direction: 'reduce', minPct: -50, maxPct: 0 },
    { id: 'mental_disorder', name: '限制行为能力精神病人', direction: 'reduce', minPct: -40, maxPct: 0 },
    { id: 'attempted',       name: '未遂犯',              direction: 'reduce', minPct: -50, maxPct: 0 },
    { id: 'accessory',       name: '从犯',                direction: 'reduce', minPct: -50, maxPct: -20 },
    { id: 'coerced',         name: '胁从犯',              direction: 'reduce', minPct: -60, maxPct: -30 },
    { id: 'solicitor',       name: '教唆犯',              direction: 'reduce', minPct: -30, maxPct: 0 },
    { id: 'suspension',      name: '中止犯',              direction: 'reduce', minPct: -100, maxPct: -50 },
    { id: 'preparation',     name: '预备犯',              direction: 'reduce', minPct: -50, maxPct: 0 },
    { id: 'excessive_defense', name: '防卫过当',          direction: 'reduce', minPct: -60, maxPct: -20 },
    { id: 'excessive_escape',  name: '避险过当',          direction: 'reduce', minPct: -60, maxPct: -20 },
  ],

  // 一般情节（后调节，相加）
  generalFactors: [
    // 从宽
    { id: 'surrender',       name: '自首',            direction: 'reduce', minPct: -40, maxPct: 0 },
    { id: 'confess',         name: '坦白',            direction: 'reduce', minPct: -20, maxPct: 0 },
    { id: 'confess_heavy',   name: '坦白（供述未掌握同种较重罪）', direction: 'reduce', minPct: -30, maxPct: -10 },
    { id: 'confess_avoid',   name: '坦白（避免严重后果）', direction: 'reduce', minPct: -50, maxPct: -30 },
    { id: 'court_guilty',    name: '当庭认罪',        direction: 'reduce', minPct: -10, maxPct: 0 },
    { id: 'merit',           name: '一般立功',        direction: 'reduce', minPct: -20, maxPct: 0 },
    { id: 'major_merit',     name: '重大立功',        direction: 'reduce', minPct: -50, maxPct: -20 },
    { id: 'restitution',     name: '退赃退赔',        direction: 'reduce', minPct: -30, maxPct: 0 },
    { id: 'compensation_forgiven', name: '赔偿谅解',  direction: 'reduce', minPct: -40, maxPct: 0 },
    { id: 'forgiven_no_compensation', name: '未赔偿但谅解', direction: 'reduce', minPct: -20, maxPct: 0 },
    { id: 'reconciliation',  name: '刑事和解',        direction: 'reduce', minPct: -50, maxPct: 0 },
    { id: 'good_detention',  name: '羁押表现好',      direction: 'reduce', minPct: -10, maxPct: 0 },
    { id: 'plea_guilty',     name: '认罪认罚',        direction: 'reduce', minPct: -30, maxPct: 0 },
    { id: 'victim_fault',    name: '被害人过错',      direction: 'reduce', minPct: -40, maxPct: 0 },
    { id: 'civil_dispute',   name: '民间矛盾',        direction: 'reduce', minPct: -30, maxPct: 0 },
    // 从重
    { id: 'recidivist',      name: '累犯',            direction: 'increase', minPct: 10,  maxPct: 40, fixMonths: 3 },
    { id: 'prior_crime',     name: '前科',            direction: 'increase', minPct: 0,   maxPct: 10 },
    { id: 'vulnerable_target', name: '弱势对象',      direction: 'increase', minPct: 0,   maxPct: 20 },
    { id: 'disaster_crime',  name: '灾害期间犯罪',    direction: 'increase', minPct: 0,   maxPct: 20 },
  ],

  // ==================== 自然语言关键词 ====================
  keywords: {
    // 罪名关键词（顺序重要：合同诈骗必须在诈骗前面）
    crime: [
      ['合同诈骗罪', ['合同诈骗', '合同欺诈', '签订合同履行过程中骗取']],
      ['盗窃罪', ['盗窃', '偷窃', '偷了', '偷走', '盗取']],
      ['诈骗罪', ['诈骗', '骗了', '骗取', '欺诈', '电信诈骗', '网络诈骗']],
      ['职务侵占罪', ['职务侵占', '利用职务便利侵占', '侵占公司财物', '挪用公司资金']],
      ['寻衅滋事罪', ['寻衅滋事', '随意殴打', '追逐拦截', '强拿硬要']],
      ['危险驾驶罪', ['危险驾驶', '醉驾', '醉酒驾驶', '酒驾驾车']],
      ['交通肇事罪', ['交通肇事', '交通事故致人死亡', '违章驾驶造成事故']],
      ['非国家工作人员受贿罪', ['非国家工作人员受贿', '公司人员受贿', '商业贿赂']],
      ['贪污罪', ['贪污', '侵吞公共财物', '利用职务便利非法占有公共财物']],
      ['受贿罪', ['受贿', '收受他人财物', '权钱交易']],
      ['故意伤害罪', ['故意伤害', '打伤', '打成', '殴打导致', '故意损害他人身体健康', '轻伤', '重伤']],
      ['抢劫罪', ['抢劫', '持刀抢劫', '拦路抢劫', '入室抢劫', '暴力劫取']],
      ['帮助信息网络犯罪活动罪', ['帮信', '帮助信息网络', '信息网络犯罪', '提供银行卡', '跑分', '支付结算帮助', '技术支持']],
    ],
    // 地区关键词
    region: [
      ['一类地区', ['广州', '深圳', '珠海', '佛山', '中山', '东莞']],
      ['二类地区', ['韶关', '河源', '梅州', '汕尾', '阳江', '湛江', '茂名', '肇庆', '清远', '潮州', '揭阳', '云浮']],
    ],
    // 情节关键词
    factor: {
      '自首': ['自首', '主动投案', '自动投案并如实供述'],
      '坦白': ['坦白', '如实供述', '到案后如实供述', '认罪态度好'],
      '当庭认罪': ['当庭认罪', '自愿认罪', '认罪'],
      '一般立功': ['立功', '一般立功', '揭发他人犯罪', '提供重要线索'],
      '重大立功': ['重大立功', '重大立功表现'],
      '退赃退赔': ['退赃', '退赔', '退还赃款', '返还财物'],
      '赔偿谅解': ['赔偿', '谅解', '赔偿被害人损失并取得谅解', '积极赔偿'],
      '认罪认罚': ['认罪认罚'],
      '被害人过错': ['被害人有过错', '被害人挑衅', '被害人先动手'],
      '累犯': ['累犯', '曾经故意犯罪'],
      '从犯': ['从犯', '起次要作用', '起辅助作用', '帮助犯'],
      '未遂犯': ['未遂', '犯罪未遂', '未能得逞'],
    },
    // 电信诈骗关键词
    telecom: ['电信', '网络诈骗', '刷单', '杀猪盘', '冒充'],
  },
};

// ==================== 罪名ID映射 ====================
export const CRIME_NAME_TO_ID: Record<CrimeName, CrimeId> = {
  '盗窃罪': 'theft',
  '诈骗罪': 'fraud',
  '合同诈骗罪': 'contract_fraud',
  '职务侵占罪': 'embezzlement',
  '寻衅滋事罪': 'disturbance',
  '危险驾驶罪': 'dangerous_driving',
  '交通肇事罪': 'traffic_accident',
  '非国家工作人员受贿罪': 'non_official_bribery',
  '贪污罪': 'corruption',
  '受贿罪': 'bribery',
  '故意伤害罪': 'intentional_injury',
  '抢劫罪': 'robbery',
  '帮助信息网络犯罪活动罪': 'aiding_cyber_crime',
};

/** 来源应用 `app.js#getLawRefs` 的13罪名法律依据展示映射。 */
export const LEGAL_REFERENCES: Record<CrimeName, readonly string[]> = {
  "盗窃罪": [
    "最高法量刑指导意见（法发〔2021〕21号）",
    "广东省量刑实施细则（粤高法发〔2017〕6号）",
    "盗窃罪三类标准：较大3千~10万（一类）/2千~10万（二类），巨大10~50万，特别巨大>50万",
  ],
  "诈骗罪": [
    "最高法、最高检量刑指导意见（法发〔2021〕21号）",
    "广东省量刑实施细则（粤高法发〔2017〕6号）",
    "电信网络诈骗数额档位：法发〔2016〕32号",
  ],
  "合同诈骗罪": [
    "最高法量刑指导意见（法发〔2021〕21号）",
    "广东省量刑实施细则",
    "个人：较大2~30万（一类），巨大30~150万，特别巨大>150万",
  ],
  "职务侵占罪": [
    "《刑法》第271条；法释〔2026〕6号第8条",
    "2026年5月1日起参照贪污罪定罪量刑标准",
    "行为时间与司法解释时间效力须人工复核",
  ],
  "寻衅滋事罪": ["《刑法》第293条", "法释〔2013〕18号", "一次：3年以下，三次以上纠集：5~7年"],
  "危险驾驶罪": ["《刑法》第133条之一", "2023年四部门意见", "1~6个月拘役"],
  "交通肇事罪": ["《刑法》第133条", "基本：2年以下，逃逸：3~5年，逃逸致死：7~10年"],
  "非国家工作人员受贿罪": [
    "《刑法》第163条；法释〔2026〕6号第8条",
    "2026年5月1日起参照受贿罪定罪量刑标准",
    "行为时间与司法解释时间效力须人工复核",
  ],
  "贪污罪": ["《刑法》第382条", "较大3~20万：3年以下，巨大20~300万：3~10年，特别巨大>300万：10年以上~死刑"],
  "受贿罪": ["《刑法》第385条", "同贪污罪标准"],
  "故意伤害罪": ["《刑法》第234条", "基本情形：3年以下；重伤：3~10年；致死或以特别残忍手段致严重残疾：10年以上、无期徒刑或死刑"],
  "抢劫罪": ["《刑法》第263条", "基本：3~10年，入户/持枪/致人重伤等加重情形：10年以上~死刑"],
  "帮助信息网络犯罪活动罪": [
    "《刑法》第287条之二；法释〔2019〕15号第12条",
    "情节严重的，处3年以下有期徒刑或者拘役，并处或者单处罚金",
  ],
};

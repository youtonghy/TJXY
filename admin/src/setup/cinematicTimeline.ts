export const DURATION = 32;
export const CINEMATIC_DURATION_MS = DURATION * 1_000;
export const BRAND_HANDOFF_SECONDS = 29.5;

export const CHAPTERS = [
  { time: 0, phase: 'projector', year: 'THE BEGINNING', title: '光，从这里开始。', subtitle: '一束光，两只胶卷盘。让静止的瞬间，第一次流动起来。', label: '胶卷放映机', titleEn: 'It all begins with light.', subtitleEn: 'A beam of light. Two reels of film. Still moments come to life.' },
  { time: 5, phase: 'monochrome-film', year: 'BLACK & WHITE', title: '黑白之间，万千故事。', subtitle: '光与影，已经足够让我们走进另一个世界。', label: '黑白电影', titleEn: 'Two shades. Endless stories.', subtitleEn: 'Light and shadow were all we needed to enter another world.' },
  { time: 9, phase: 'colour-cinema', year: 'LIVING COLOR', title: '于是，世界有了色彩。', subtitle: '山川、落日、每一份感动，都有了自己的颜色。', label: '彩色电影', titleEn: 'Then the world found its color.', subtitleEn: 'Mountains, sunsets, and every emotion found a color of their own.' },
  { time: 13, phase: 'crt', year: 'THE TELEVISION AGE', title: '把整个世界，带回家。', subtitle: '旋动频道，微亮的显像管，成为一家人的共同记忆。', label: '显像管电视', titleEn: 'The world came home.', subtitleEn: 'A turn of the dial. A glowing screen. Memories shared by the whole family.' },
  { time: 19, phase: 'lcd', year: 'A WIDER WORLD', title: '屏幕更薄，视野更广。', subtitle: '从厚重的机身，到一扇清晰、轻盈的光影之窗。', label: '液晶电视', titleEn: 'Thinner screens. Wider horizons.', subtitleEn: 'The screen became a clear, light window into another world.' },
  { time: 24, phase: 'devices', year: 'EVERY SCREEN. EVERYWHERE.', title: '好故事，始终在你身边。', subtitle: '手机、电脑、平板。光影的下一站，就在你的手中。', label: '多端时代', titleEn: 'Your stories, always with you.', subtitleEn: 'Phone, computer, tablet. The next chapter is in your hands.' },
] as const;

export type CinematicPhase = typeof CHAPTERS[number]['phase'] | 'brand-handoff';

export interface CinematicTimelineFrame {
  elapsedMilliseconds: number;
  phase: CinematicPhase;
  phaseProgress: number;
  isComplete: boolean;
}

export function getCinematicTimelineFrame(elapsedMilliseconds: number): CinematicTimelineFrame {
  const safeElapsed = Number.isFinite(elapsedMilliseconds) ? elapsedMilliseconds : 0;
  const elapsed = Math.min(Math.max(safeElapsed, 0), CINEMATIC_DURATION_MS);
  const seconds = elapsed / 1_000;
  const index = Math.max(0, CHAPTERS.filter(chapter => seconds >= chapter.time).length - 1);
  const chapter = CHAPTERS[index] ?? CHAPTERS[0];
  const brand = seconds >= BRAND_HANDOFF_SECONDS;
  const start = brand ? BRAND_HANDOFF_SECONDS : chapter.time;
  const end = brand ? DURATION : CHAPTERS[index + 1]?.time ?? BRAND_HANDOFF_SECONDS;
  return {
    elapsedMilliseconds: elapsed,
    phase: brand ? 'brand-handoff' : chapter.phase,
    phaseProgress: (seconds - start) / (end - start),
    isComplete: elapsed === CINEMATIC_DURATION_MS,
  };
}

export function getCinematicCaption(phase: CinematicPhase, locale: 'zh-CN' | 'en-US') {
  const index = CHAPTERS.findIndex(chapter => chapter.phase === phase);
  const chapter = CHAPTERS[index] ?? CHAPTERS[CHAPTERS.length - 1] ?? CHAPTERS[0];
  const english = locale === 'en-US';
  if (phase === 'brand-handoff') return {
    number: '06', year: 'THE STORY CONTINUES',
    title: english ? 'Every screen is a new beginning.' : '每一块屏幕，都是新的开始。',
    subtitle: english ? 'TJXY · Keep your favorite stories close, wherever you are.' : 'TJXY · 让你珍藏的故事，随时随地继续。',
  };
  return {
    number: String(index + 1).padStart(2, '0'),
    year: chapter.year,
    title: english ? chapter.titleEn : chapter.title,
    subtitle: english ? chapter.subtitleEn : chapter.subtitle,
  };
}

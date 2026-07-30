/* eslint-disable @typescript-eslint/no-unnecessary-condition */
import { Alert, Button, Card, Skeleton } from '@heroui/react';
import { ArrowLeft, CircleAlert, Film } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { getItem, type MediaItem } from '../api/catalogApi';
import { getPlaybackInfo, issuePlaybackTicket, revokePlaybackTicket, type PlaybackTicket } from '../api/playbackApi';
import { selectBrowserSource } from './sourceSelection';

type PlaybackState = 'loading' | 'ready' | 'demo' | 'unsupported' | 'error';

export function PlayerPage() {
  const { id } = useParams();
  const videoRef = useRef<HTMLVideoElement>(null);
  const [item, setItem] = useState<MediaItem>();
  const [ticket, setTicket] = useState<PlaybackTicket>();
  const [state, setState] = useState<PlaybackState>('loading');

  useEffect(() => {
    if (!id) return;
    let active = true;
    void Promise.resolve()
      .then(() => {
        setState('loading');
        setItem(undefined);
        setTicket(undefined);
        return getItem(id);
      })
      .then(async (nextItem) => {
        if (!active) return;
        setItem(nextItem);
        if (nextItem.HasMediaSources === false) {
          setState('demo');
          return;
        }
        const playback = await getPlaybackInfo(id);
        if (!playback.PlaySessionId) throw new Error('missing playback session');
        const source = selectBrowserSource(playback.MediaSources ?? []);
        if (!source) {
          setState('unsupported');
          return;
        }
        const issued = await issuePlaybackTicket(id, source.Id, playback.PlaySessionId);
        if (active) {
          setTicket(issued);
          setState('ready');
        }
      })
      .catch(() => {
        if (active) setState('error');
      });
    return () => {
      active = false;
    };
  }, [id]);

  useEffect(() => () => {
    if (ticket) void revokePlaybackTicket(ticket.Id).catch(() => undefined);
  }, [ticket]);

  if (!id) return <p className="text-muted">This title could not be found.</p>;
  if (state === 'loading') return <Skeleton className="aspect-video w-full rounded-2xl" />;
  if (state === 'error') return <PlaybackAlert id={id} status="danger" title="Playback unavailable">We could not prepare this title right now.</PlaybackAlert>;
  if (state === 'demo') {
    return (
      <div className="space-y-5">
        <BackLink id={id} />
        <Alert status="warning">
          <Alert.Indicator><Film className="size-4" /></Alert.Indicator>
          <Alert.Content>
            <Alert.Title>Demo title, no video file attached</Alert.Title>
            <Alert.Description>
              This development catalog contains metadata and artwork only. Add a media source in the library before starting playback.
            </Alert.Description>
          </Alert.Content>
        </Alert>
      </div>
    );
  }
  if (state === 'unsupported') {
    return (
      <div className="space-y-5">
        <BackLink id={id} />
        <PlaybackAlert id={id} status="warning" title="No browser-compatible source">
          Choose an MP4, WebM, MP3, M4A, or Ogg source for browser playback.
        </PlaybackAlert>
      </div>
    );
  }
  if (!item || !ticket) return <Skeleton className="aspect-video w-full rounded-2xl" />;

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between">
        <BackLink id={id} />
        <Button onPress={() => videoRef.current?.pause()} variant="tertiary">Exit</Button>
      </div>
      <div className="overflow-hidden rounded-2xl bg-black shadow-sm">
        <video
          aria-label={`Playing ${item.Name}`}
          className="aspect-video w-full"
          controls
          onEnded={() => { void revokePlaybackTicket(ticket.Id).catch(() => undefined); }}
          ref={videoRef}
          src={ticket.StreamUrl}
        />
      </div>
      <Card>
        <Card.Header>
          <Card.Title>{item.Name}</Card.Title>
          <Card.Description>Direct browser playback</Card.Description>
        </Card.Header>
      </Card>
    </div>
  );
}

function BackLink({ id }: { id: string }) {
  return <Link className="inline-flex items-center gap-2 text-sm text-muted hover:text-foreground" to={`/app/items/${id}`}><ArrowLeft className="size-4" />Back to details</Link>;
}

function PlaybackAlert({ id, status, title, children }: { id: string; status: 'danger' | 'warning'; title: string; children: string }) {
  return (
    <div className="space-y-5">
      <BackLink id={id} />
      <Alert status={status}>
        <Alert.Indicator><CircleAlert className="size-4" /></Alert.Indicator>
        <Alert.Content>
          <Alert.Title>{title}</Alert.Title>
          <Alert.Description>{children}</Alert.Description>
        </Alert.Content>
      </Alert>
    </div>
  );
}

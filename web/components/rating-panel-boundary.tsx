import { Component, type ReactNode } from 'react';
import { Button } from '@/components/ui/button';

export default class RatingPanelBoundary extends Component<
  { children: ReactNode },
  { failed: boolean }
> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  render() {
    if (!this.state.failed) return this.props.children;
    return (
      <section className="panel rating-panel" role="alert">
        <div className="section-heading">
          <h2>Your rating trend couldn’t load</h2>
        </div>
        <p className="panel-subtitle">
          Your games are still available. Check your connection, then reload to try again.
        </p>
        <Button className="primary-button" onClick={() => window.location.reload()}>
          Reload Tempo
        </Button>
      </section>
    );
  }
}

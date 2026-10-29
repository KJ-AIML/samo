import React, { useState } from "react";
import { CATEGORY_COLORS, PackedBall, formatBytes } from "../types";

interface BallsCanvasProps {
  balls: PackedBall[];
  selectedBallId: string | null;
  onSelectBall: (ball: PackedBall) => void;
}

export const BallsCanvas: React.FC<BallsCanvasProps> = ({
  balls,
  selectedBallId,
  onSelectBall,
}) => {
  const [hovered, setHovered] = useState<PackedBall | null>(null);

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: "18px",
        position: "relative",
        minHeight: "420px",
      }}
    >
      <div
        style={{
          width: "100%",
          maxWidth: "680px",
          aspectRatio: "1.08 / 1",
          background: "var(--bg-canvas)",
          borderRadius: "6px",
          border: "1px solid var(--border-color)",
          position: "relative",
          overflow: "hidden",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        {balls.length === 0 ? (
          <div style={{ color: "var(--text-secondary)", fontSize: "14px" }}>
            Click <strong>Start</strong> to scan a folder and render the Disk Space ball map.
          </div>
        ) : (
          <svg
            viewBox="0 0 1000 1000"
            style={{ width: "100%", height: "100%", display: "block" }}
          >
            <defs>
              {(Object.keys(CATEGORY_COLORS) as Array<keyof typeof CATEGORY_COLORS>).map(
                (cat) => {
                  const baseColor = CATEGORY_COLORS[cat];
                  return (
                    <radialGradient
                      key={cat}
                      id={`grad-${cat}`}
                      cx="35%"
                      cy="30%"
                      r="70%"
                    >
                      <stop offset="0%" stopColor="#ffffff" stopOpacity="0.55" />
                      <stop offset="28%" stopColor={baseColor} stopOpacity="1" />
                      <stop offset="85%" stopColor={baseColor} stopOpacity="0.88" />
                      <stop offset="100%" stopColor="#05070c" stopOpacity="0.92" />
                    </radialGradient>
                  );
                }
              )}
            </defs>

            {balls.map((ball) => {
              const isSelected = selectedBallId === ball.id;
              const isHovered = hovered?.id === ball.id;
              return (
                <g
                  key={ball.id}
                  style={{ cursor: "pointer" }}
                  onClick={() => onSelectBall(ball)}
                  onMouseEnter={() => setHovered(ball)}
                  onMouseLeave={() => setHovered(null)}
                >
                  <circle
                    cx={ball.x}
                    cy={ball.y}
                    r={ball.r}
                    fill={`url(#grad-${ball.category})`}
                    stroke={
                      isSelected
                        ? "#ffffff"
                        : isHovered
                        ? "rgba(255,255,255,0.75)"
                        : "rgba(12,16,24,0.75)"
                    }
                    strokeWidth={isSelected ? 5 : isHovered ? 3 : 2}
                  />
                  {ball.r >= 48 && (
                    <>
                      <text
                        x={ball.x}
                        y={ball.y - 4}
                        textAnchor="middle"
                        fill="#ffffff"
                        fontSize={Math.min(24, Math.max(12, ball.r * 0.21))}
                        fontWeight={700}
                        style={{
                          pointerEvents: "none",
                          textShadow: "0 1px 4px rgba(0,0,0,0.85)",
                        }}
                      >
                        {ball.label.length > 16
                          ? `${ball.label.slice(0, 15)}…`
                          : ball.label}
                      </text>
                      <text
                        x={ball.x}
                        y={ball.y + Math.min(22, Math.max(14, ball.r * 0.19))}
                        textAnchor="middle"
                        fill="rgba(255,255,255,0.9)"
                        fontSize={Math.min(18, Math.max(11, ball.r * 0.16))}
                        fontFamily="SF Mono, Menlo, monospace"
                        style={{
                          pointerEvents: "none",
                          textShadow: "0 1px 4px rgba(0,0,0,0.85)",
                        }}
                      >
                        {formatBytes(ball.bytes)}
                      </text>
                    </>
                  )}
                </g>
              );
            })}
          </svg>
        )}

        {hovered && (
          <div
            style={{
              position: "absolute",
              bottom: "12px",
              left: "12px",
              right: "12px",
              background: "rgba(15, 18, 25, 0.92)",
              border: "1px solid var(--border-color)",
              borderRadius: "6px",
              padding: "8px 12px",
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              fontSize: "12.5px",
              pointerEvents: "none",
            }}
          >
            <div>
              <strong>{hovered.label}</strong>{" "}
              <span style={{ color: "var(--text-secondary)", marginLeft: "6px" }}>
                {hovered.path}
              </span>
            </div>
            <div className="mono-font" style={{ fontWeight: 700 }}>
              {formatBytes(hovered.bytes)}
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

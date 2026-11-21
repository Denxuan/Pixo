import { emit } from "@tauri-apps/api/event";

type CameraWindow = Window & { __pixoCameraDevice?: string };

function initialCameraDevice() {
  const injected = (window as CameraWindow).__pixoCameraDevice;
  if (typeof injected === "string") return injected;
  return new URLSearchParams(window.location.search).get("device") ?? "";
}

function cameraVideo(deviceId: string): MediaTrackConstraints {
  if (deviceId) return { deviceId: { exact: deviceId }, width: { ideal: 640 }, height: { ideal: 640 } };
  return { facingMode: "user", width: { ideal: 640 }, height: { ideal: 640 } };
}

export default function ScreenshotCamera() {
  const { t } = useTranslation();
  const videoRef = useRef<HTMLVideoElement>(null);
  const activeDevice = useRef<string | null>(null);
  const [deviceId, setDeviceId] = useState(initialCameraDevice);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    const nodes = [document.documentElement, document.body];
    const previous = nodes.map((node) => node.style.background);
    for (const node of nodes) node.style.background = "transparent";
    return () => {
      nodes.forEach((node, index) => {
        node.style.background = previous[index] ?? "";
      });
    };
  }, []);

  useEffect(() => {
    const onDevice = (event: Event) => {
      const detail = (event as CustomEvent<unknown>).detail;
      if (typeof detail !== "string") return;
      if (detail === activeDevice.current) {
        void emit("screenshot://camera", { ready: true });
        return;
      }
      setFailed(false);
      setDeviceId(detail);
    };
    window.addEventListener("pixo-camera", onDevice);
    return () => window.removeEventListener("pixo-camera", onDevice);
  }, []);

  useEffect(() => {
    let stream: MediaStream | null = null;
    let alive = true;
    activeDevice.current = null;
    setFailed(false);
    const devices = navigator.mediaDevices;
    if (!devices?.getUserMedia) {
      setFailed(true);
      void emit("screenshot://camera", { ready: false });
      return;
    }
    void devices.getUserMedia({
      audio: false,
      video: cameraVideo(deviceId),
    }).then((next) => {
      if (!alive) {
        next.getTracks().forEach((track) => track.stop());
        return;
      }
      stream = next;
      activeDevice.current = deviceId;
      const video = videoRef.current;
      if (video) {
        video.srcObject = next;
        void video.play().catch(() => undefined);
      }
      void emit("screenshot://camera", { ready: true });
    }).catch(() => {
      if (!alive) return;
      setFailed(true);
      void emit("screenshot://camera", { ready: false });
    });
    return () => {
      alive = false;
      activeDevice.current = null;
      stream?.getTracks().forEach((track) => track.stop());
    };
  }, [deviceId]);

  return (
    <div className="h-full w-full overflow-hidden rounded-2xl bg-black">
      <video
        ref={videoRef}
        className="h-full w-full object-cover"
        style={{ transform: "scaleX(-1)" }}
        muted
        playsInline
        autoPlay
        aria-label={t("zdx.label.screenshot.recordCamera")}
      />
      {failed && (
        <div className="absolute inset-0 flex items-center justify-center p-3 text-center text-xs leading-5 text-white">
          {t("zdx.text.screenshot.cameraFailed")}
        </div>
      )}
    </div>
  );
}

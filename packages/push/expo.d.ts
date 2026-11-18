/** The app.json field used by Expo's token exchange. */
export interface ExpoPushApp {
  extra: { eas: { projectId: string } };
}

export interface ExpoPushSubscription {
  apple?: { deviceToken: string; appId: string; installationId: string } | null;
  android?: { installationId: string; token: string; appId: string } | null;
}

export interface ExpoPushOptions {
  /**
   * Required for an APNs token: whether the build registers with APNs's sandbox. True on a simulator and under a
   * development profile; false under an Ad Hoc, TestFlight or App Store one.
   */
  development?: boolean;
}

export declare function getExpoPushTokenAsync(subscription: ExpoPushSubscription, app: ExpoPushApp, options?: ExpoPushOptions): Promise<{ type: 'expo'; data: string }>;

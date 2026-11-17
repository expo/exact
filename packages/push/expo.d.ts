/** The app.json field used by Expo's token exchange. */
export interface ExpoPushApp {
  extra: { eas: { projectId: string } };
}

export interface ExpoPushSubscription {
  android?: { installationId: string; token: string; appId: string } | null;
}

export declare function getExpoPushTokenAsync(subscription: ExpoPushSubscription, app: ExpoPushApp): Promise<{ type: 'expo'; data: string }>;

# Stripe Integration Implementation Plan

Based on comprehensive research of the Helix CLI, Helix Cloud, and Dashboard Cloud codebases, this document outlines the implementation plan for integrating Stripe payments into the Helix ecosystem.

## Current Architecture Overview

### Authentication Flow
1. **CLI (helix-cli)**: Uses GitHub device flow via WebSocket connection
   - Manual browser opening (terminal hyperlinks)
   - Stores API keys in `~/.helix/credentials`
   - No automatic browser launching capability

2. **Backend (helix-cloud)**: 
   - WebSocket endpoint at `/login` for device flow
   - Uses Unkey for API key management
   - Stores users in HelixDB with minimal schema
   - No existing billing infrastructure

3. **Frontend (dashboard-cloud)**:
   - Next.js 15 with better-auth library
   - GitHub OAuth for web authentication
   - Cookie-based session management
   - Pricing UI exists but no payment processing

## Database Schema Changes

### 1. Update User Model in HelixDB

```graphql
# Extend existing User type in helix-cloud
type User {
  id: String!
  gh_id: Int!
  gh_login: String!
  email: String!
  name: String!
  
  # New billing fields
  stripe_customer_id: String
  billing_status: String # "active", "inactive", "trial", "past_due"
  subscription_id: String
  subscription_tier: String # "hx-10", "hx-20", etc.
  trial_ends_at: DateTime
  billing_email: String
  payment_method_last4: String
  payment_method_brand: String
}

# New type for billing events
type BillingEvent {
  id: String!
  user_id: String!
  event_type: String! # "subscription_created", "payment_succeeded", etc.
  stripe_event_id: String!
  metadata: JSON
  created_at: DateTime!
}

# CLI session tokens for auth flow
type CLIAuthSession {
  id: String!
  token: String!
  user_id: String
  status: String! # "pending", "authenticated", "billing_complete", "expired"
  redirect_after_auth: String
  expires_at: DateTime!
  created_at: DateTime!
}
```

### 2. Database Migration Strategy
- Add fields to existing User records with nullable defaults
- Create new tables for BillingEvent and CLIAuthSession
- Index stripe_customer_id for webhook lookups

## Implementation Plan

### Phase 1: CLI Browser Integration

#### 1.1 Add Browser Opening Capability
**File**: `helix-cli/Cargo.toml`
```toml
[dependencies]
webbrowser = "1.0"
uuid = { version = "1.0", features = ["v4"] }
```

#### 1.2 Modify Login Command
**File**: `helix-cli/src/commands/auth.rs`

```rust
pub async fn login(setup_billing: bool) -> Result<()> {
    if setup_billing {
        browser_based_login(true).await
    } else {
        // Keep existing device flow as default
        let (key, user_id) = github_login().await?;
        let creds = Credentials {
            helix_admin_key: key,
            user_id,
        };
        creds.write_to_file()?;
        Ok(())
    }
}

async fn browser_based_login(redirect_to_billing: bool) -> Result<()> {
    // Generate secure session token
    let session_token = uuid::Uuid::new_v4().to_string();
    
    // Create session on backend
    let client = reqwest::Client::new();
    let cloud_url = format!("http://{}/api/auth/cli-session", *CLOUD_AUTHORITY);
    
    let response = client.post(&cloud_url)
        .json(&json!({
            "token": session_token,
            "redirect_after_auth": if redirect_to_billing { "/billing/setup" } else { "/dashboard" }
        }))
        .send()
        .await?;
    
    // Open browser to dashboard
    let dashboard_url = std::env::var("DASHBOARD_URL")
        .unwrap_or_else(|_| "https://dashboard.helixdb.ai".to_string());
    
    let auth_url = format!(
        "{}/auth/cli-login?token={}",
        dashboard_url,
        session_token
    );
    
    println!("Opening browser for authentication...");
    webbrowser::open(&auth_url)?;
    
    // Poll for completion
    poll_for_auth_completion(&session_token).await?;
    
    Ok(())
}

async fn poll_for_auth_completion(token: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let poll_url = format!("http://{}/api/auth/cli-session/{}/status", *CLOUD_AUTHORITY, token);
    
    println!("Waiting for authentication to complete...");
    
    for _ in 0..60 { // Poll for up to 10 minutes
        tokio::time::sleep(Duration::from_secs(10)).await;
        
        let response = client.get(&poll_url).send().await?;
        
        if response.status().is_success() {
            let status: serde_json::Value = response.json().await?;
            
            match status["status"].as_str() {
                Some("billing_complete") | Some("authenticated") => {
                    // Fetch credentials
                    let creds_url = format!("http://{}/api/auth/cli-session/{}/credentials", 
                                          *CLOUD_AUTHORITY, token);
                    let creds_response = client.get(&creds_url).send().await?;
                    
                    if creds_response.status().is_success() {
                        let creds: Credentials = creds_response.json().await?;
                        creds.write_to_file()?;
                        
                        println!("✓ Authentication successful!");
                        if status["billing_status"].as_str() == Some("active") {
                            println!("✓ Billing setup complete!");
                        }
                        return Ok(());
                    }
                }
                Some("expired") => {
                    return Err(anyhow!("Authentication session expired"));
                }
                _ => continue,
            }
        }
    }
    
    Err(anyhow!("Authentication timeout - please try again"))
}
```

#### 1.3 Update CLI Parser
**File**: `helix-cli/src/main.rs`
```rust
#[derive(Parser, Debug)]
enum Commands {
    Login {
        #[arg(long, help = "Setup billing after authentication")]
        setup_billing: bool,
    },
    // ... other commands
}
```

### Phase 2: Backend API Implementation

#### 2.1 Create CLI Session Endpoints
**File**: `helix-cloud/cloud-server/src/routes/cli_auth.rs`

```rust
use axum::{Json, extract::Path, http::StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct CreateSessionRequest {
    token: String,
    redirect_after_auth: Option<String>,
}

#[derive(Serialize)]
pub struct SessionStatus {
    status: String,
    user_id: Option<String>,
    billing_status: Option<String>,
}

pub async fn create_cli_session(
    Json(req): Json<CreateSessionRequest>,
) -> Result<StatusCode, AppError> {
    // Create session in database
    let session_id = uuid::Uuid::new_v4().to_string();
    let expires_at = chrono::Utc::now() + chrono::Duration::minutes(15);
    
    // Store in HelixDB
    helix_db::create_cli_session(CLIAuthSession {
        id: session_id,
        token: req.token,
        status: "pending".to_string(),
        redirect_after_auth: req.redirect_after_auth,
        expires_at,
        created_at: chrono::Utc::now(),
        user_id: None,
    }).await?;
    
    Ok(StatusCode::CREATED)
}

pub async fn get_session_status(
    Path(token): Path<String>,
) -> Result<Json<SessionStatus>, AppError> {
    let session = helix_db::get_cli_session_by_token(&token).await?;
    
    let billing_status = if let Some(user_id) = &session.user_id {
        let user = helix_db::get_user(user_id).await?;
        user.billing_status
    } else {
        None
    };
    
    Ok(Json(SessionStatus {
        status: session.status,
        user_id: session.user_id,
        billing_status,
    }))
}

pub async fn get_session_credentials(
    Path(token): Path<String>,
) -> Result<Json<Credentials>, AppError> {
    let session = helix_db::get_cli_session_by_token(&token).await?;
    
    if session.status != "authenticated" && session.status != "billing_complete" {
        return Err(AppError::Unauthorized);
    }
    
    let user_id = session.user_id.ok_or(AppError::Unauthorized)?;
    
    // Get or create API key for user
    let api_key = get_or_create_api_key(&user_id).await?;
    
    Ok(Json(Credentials {
        user_id,
        helix_admin_key: api_key,
    }))
}
```

#### 2.2 Stripe Integration Service
**File**: `helix-cloud/cloud-server/src/services/stripe.rs`

```rust
use stripe::{
    Client, Customer, CreateCustomer, CreateCheckoutSession,
    CreateCheckoutSessionItems, Currency, CheckoutSession,
};

pub struct StripeService {
    client: Client,
}

impl StripeService {
    pub fn new(secret_key: String) -> Self {
        Self {
            client: Client::new(secret_key),
        }
    }
    
    pub async fn create_or_get_customer(&self, user: &User) -> Result<Customer> {
        if let Some(customer_id) = &user.stripe_customer_id {
            // Retrieve existing customer
            Customer::retrieve(&self.client, customer_id, &[]).await
        } else {
            // Create new customer
            let customer = Customer::create(
                &self.client,
                CreateCustomer {
                    email: Some(&user.email),
                    name: Some(&user.name),
                    metadata: Some(std::collections::HashMap::from([
                        ("helix_user_id".to_string(), user.id.clone()),
                        ("github_id".to_string(), user.gh_id.to_string()),
                    ])),
                    ..Default::default()
                },
            ).await?;
            
            // Update user with customer ID
            helix_db::update_user_stripe_customer_id(&user.id, &customer.id).await?;
            
            Ok(customer)
        }
    }
    
    pub async fn create_checkout_session(
        &self,
        customer_id: &str,
        price_id: &str,
        success_url: &str,
        cancel_url: &str,
        cli_token: Option<&str>,
    ) -> Result<CheckoutSession> {
        let mut metadata = std::collections::HashMap::new();
        if let Some(token) = cli_token {
            metadata.insert("cli_token".to_string(), token.to_string());
        }
        
        CheckoutSession::create(
            &self.client,
            CreateCheckoutSession {
                customer: Some(customer_id.to_string()),
                line_items: Some(vec![CreateCheckoutSessionItems {
                    price: Some(price_id.to_string()),
                    quantity: Some(1),
                    ..Default::default()
                }]),
                mode: Some(stripe::CheckoutSessionMode::Subscription),
                success_url: Some(success_url),
                cancel_url: Some(cancel_url),
                metadata: Some(metadata),
                ..Default::default()
            },
        ).await
    }
}
```

#### 2.3 Stripe Webhook Handler
**File**: `helix-cloud/cloud-server/src/routes/stripe_webhook.rs`

```rust
use stripe::{Event, EventObject, EventType, Webhook};
use axum::{body::Bytes, http::HeaderMap};

pub async fn handle_stripe_webhook(
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let stripe_signature = headers
        .get("stripe-signature")
        .ok_or(AppError::BadRequest)?
        .to_str()?;
    
    let webhook_secret = std::env::var("STRIPE_WEBHOOK_SECRET")?;
    
    // Verify webhook signature
    let event = Webhook::construct_event(
        std::str::from_utf8(&body)?,
        stripe_signature,
        &webhook_secret,
    )?;
    
    match event.type_ {
        EventType::CheckoutSessionCompleted => {
            if let EventObject::CheckoutSession(session) = event.data.object {
                handle_checkout_completed(session).await?;
            }
        }
        EventType::CustomerSubscriptionCreated |
        EventType::CustomerSubscriptionUpdated => {
            if let EventObject::Subscription(sub) = event.data.object {
                handle_subscription_change(sub).await?;
            }
        }
        EventType::CustomerSubscriptionDeleted => {
            if let EventObject::Subscription(sub) = event.data.object {
                handle_subscription_cancelled(sub).await?;
            }
        }
        _ => {
            // Log unhandled event types
        }
    }
    
    Ok(StatusCode::OK)
}

async fn handle_checkout_completed(session: CheckoutSession) -> Result<()> {
    // Update user billing status
    if let Some(customer_id) = session.customer {
        let user = helix_db::get_user_by_stripe_customer_id(&customer_id).await?;
        
        helix_db::update_user_billing_status(&user.id, "active").await?;
        
        // Update CLI session if present
        if let Some(cli_token) = session.metadata.get("cli_token") {
            helix_db::update_cli_session_status(cli_token, "billing_complete").await?;
        }
        
        // Log billing event
        helix_db::create_billing_event(BillingEvent {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user.id,
            event_type: "subscription_created".to_string(),
            stripe_event_id: session.id,
            metadata: serde_json::to_value(&session)?,
            created_at: chrono::Utc::now(),
        }).await?;
    }
    
    Ok(())
}
```

### Phase 3: Frontend Implementation

#### 3.1 CLI Login Route
**File**: `dashboard-cloud/src/app/auth/cli-login/page.tsx`

```typescript
'use client'

import { useEffect } from 'react'
import { useRouter, useSearchParams } from 'next/navigation'
import { authClient } from '@/lib/auth-client'

export default function CLILoginPage() {
  const router = useRouter()
  const searchParams = useSearchParams()
  const token = searchParams.get('token')
  const { data: session, isPending } = authClient.useSession()
  const user = session?.user as HelixUser

  useEffect(() => {
    if (!token) {
      router.push('/dashboard/login')
      return
    }

    const handleCLIAuth = async () => {
      if (!user) {
        // Store CLI token and redirect to GitHub login
        sessionStorage.setItem('cli_token', token)
        router.push('/dashboard/login?cli_auth=true')
        return
      }

      // User is authenticated, update CLI session
      try {
        const response = await fetch('/api/auth/cli-callback', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ token, userId: user.id }),
          credentials: 'include',
        })

        if (response.ok) {
          const data = await response.json()
          
          if (data.redirect === '/billing/setup') {
            router.push('/dashboard/billing/setup')
          } else {
            router.push('/dashboard?cli_auth=success')
          }
        }
      } catch (error) {
        console.error('CLI auth error:', error)
        router.push('/dashboard/login?error=cli_auth_failed')
      }
    }

    if (!isPending) {
      handleCLIAuth()
    }
  }, [user, token, isPending, router])

  return (
    <div className="flex min-h-screen items-center justify-center">
      <div className="text-center">
        <h2 className="text-2xl font-bold mb-4">Authenticating CLI...</h2>
        <p className="text-gray-600">Please wait while we complete your authentication.</p>
      </div>
    </div>
  )
}
```

#### 3.2 Billing Setup Page
**File**: `dashboard-cloud/src/app/dashboard/billing/setup/page.tsx`

```typescript
'use client'

import { useState, useEffect } from 'react'
import { loadStripe } from '@stripe/stripe-js'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'

const stripePromise = loadStripe(process.env.NEXT_PUBLIC_STRIPE_PUBLISHABLE_KEY!)

const PRICING_TIERS = [
  { id: 'hx-10', name: 'HX-10', price: 39, priceId: 'price_hx10' },
  { id: 'hx-20', name: 'HX-20', price: 79, priceId: 'price_hx20' },
  { id: 'hx-40', name: 'HX-40', price: 149, priceId: 'price_hx40' },
  { id: 'hx-80', name: 'HX-80', price: 299, priceId: 'price_hx80' },
  { id: 'hx-160', name: 'HX-160', price: 499, priceId: 'price_hx160' },
  { id: 'hx-320', name: 'HX-320', price: 699, priceId: 'price_hx320' },
]

export default function BillingSetupPage() {
  const [selectedTier, setSelectedTier] = useState('hx-20')
  const [isLoading, setIsLoading] = useState(false)

  const handleSubscribe = async () => {
    setIsLoading(true)
    
    try {
      const tier = PRICING_TIERS.find(t => t.id === selectedTier)
      if (!tier) return

      // Create checkout session
      const response = await fetch('/api/billing/create-checkout', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ 
          priceId: tier.priceId,
          cliToken: sessionStorage.getItem('cli_token'),
        }),
        credentials: 'include',
      })

      if (!response.ok) throw new Error('Failed to create checkout session')

      const { sessionId } = await response.json()
      
      // Redirect to Stripe
      const stripe = await stripePromise
      const { error } = await stripe!.redirectToCheckout({ sessionId })
      
      if (error) {
        console.error('Stripe error:', error)
      }
    } catch (error) {
      console.error('Subscription error:', error)
    } finally {
      setIsLoading(false)
    }
  }

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      <h1 className="text-3xl font-bold mb-8">Choose Your Plan</h1>
      
      <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-8">
        {PRICING_TIERS.map((tier) => (
          <Card 
            key={tier.id}
            className={`p-6 cursor-pointer border-2 ${
              selectedTier === tier.id ? 'border-blue-500' : 'border-gray-200'
            }`}
            onClick={() => setSelectedTier(tier.id)}
          >
            <h3 className="text-xl font-semibold mb-2">{tier.name}</h3>
            <p className="text-3xl font-bold mb-4">${tier.price}/mo</p>
            <ul className="space-y-2 text-sm text-gray-600">
              <li>✓ Unlimited queries</li>
              <li>✓ {tier.name} performance tier</li>
              <li>✓ 24/7 support</li>
              <li>✓ API access</li>
            </ul>
          </Card>
        ))}
      </div>

      <div className="text-center">
        <Button 
          size="lg" 
          onClick={handleSubscribe}
          disabled={isLoading}
        >
          {isLoading ? 'Processing...' : 'Continue to Payment'}
        </Button>
        
        <p className="mt-4 text-sm text-gray-600">
          You'll be redirected to Stripe to complete your subscription.
          Cancel anytime.
        </p>
      </div>
    </div>
  )
}
```

#### 3.3 API Route for Checkout
**File**: `dashboard-cloud/src/app/api/billing/create-checkout/route.ts`

```typescript
import { auth } from '@/lib/auth'
import { headers } from 'next/headers'

export async function POST(request: Request) {
  const session = await auth.api.getSession({ headers: await headers() })
  
  if (!session?.user) {
    return Response.json({ error: 'Unauthorized' }, { status: 401 })
  }

  const { priceId, cliToken } = await request.json()
  const user = session.user as HelixUser

  try {
    // Call backend to create Stripe checkout session
    const response = await fetch(`${process.env.CLOUD_API_URL}/api/billing/create-checkout`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${process.env.CLOUD_API_KEY}`,
      },
      body: JSON.stringify({
        userId: user.id,
        priceId,
        successUrl: `${process.env.NEXT_PUBLIC_APP_URL}/dashboard/billing/success`,
        cancelUrl: `${process.env.NEXT_PUBLIC_APP_URL}/dashboard/billing/setup`,
        cliToken,
      }),
    })

    const data = await response.json()
    return Response.json(data)
  } catch (error) {
    console.error('Checkout creation error:', error)
    return Response.json({ error: 'Failed to create checkout session' }, { status: 500 })
  }
}
```

### Phase 4: Environment Configuration

#### 4.1 CLI Environment Variables
```bash
# .env file for helix-cli
DASHBOARD_URL=https://dashboard.helixdb.ai
CLOUD_AUTHORITY=api.helixdb.ai:3000
```

#### 4.2 Backend Environment Variables
```bash
# helix-cloud environment
STRIPE_SECRET_KEY=sk_live_xxx
STRIPE_WEBHOOK_SECRET=whsec_xxx
DASHBOARD_URL=https://dashboard.helixdb.ai

# Stripe Price IDs
STRIPE_PRICE_HX10=price_xxx
STRIPE_PRICE_HX20=price_xxx
STRIPE_PRICE_HX40=price_xxx
STRIPE_PRICE_HX80=price_xxx
STRIPE_PRICE_HX160=price_xxx
STRIPE_PRICE_HX320=price_xxx
```

#### 4.3 Frontend Environment Variables
```bash
# dashboard-cloud environment
NEXT_PUBLIC_STRIPE_PUBLISHABLE_KEY=pk_live_xxx
NEXT_PUBLIC_APP_URL=https://dashboard.helixdb.ai
CLOUD_API_URL=https://api.helixdb.ai
CLOUD_API_KEY=xxx
```

## Implementation Timeline

### Week 1: Foundation
- [ ] Database schema updates and migrations
- [ ] CLI browser integration and session token system
- [ ] Backend CLI auth endpoints

### Week 2: Stripe Integration
- [ ] Stripe service implementation in backend
- [ ] Webhook handlers and event processing
- [ ] Customer and subscription management

### Week 3: Frontend Development
- [ ] CLI login flow pages
- [ ] Billing setup and plan selection
- [ ] Success/error handling pages

### Week 4: Testing & Polish
- [ ] End-to-end testing with Stripe test mode
- [ ] Error handling improvements
- [ ] Documentation updates
- [ ] Production deployment preparation

## Security Considerations

1. **CLI Token Security**
   - Generate cryptographically secure tokens
   - 15-minute expiration time
   - Single-use tokens (invalidated after auth)
   - Proper token cleanup after timeout

2. **Stripe Webhook Security**
   - Verify webhook signatures
   - Idempotent event processing
   - Rate limiting on webhook endpoint
   - Proper error logging without exposing sensitive data

3. **Session Management**
   - Secure cookie settings for frontend
   - CSRF protection on all mutations
   - Proper session invalidation on logout
   - CLI credentials encrypted at rest

4. **API Security**
   - All billing endpoints require authentication
   - User can only access their own billing data
   - Admin endpoints for support access
   - Audit logging for all billing changes

## Testing Strategy

### Unit Tests
- CLI session token generation and validation
- Stripe service methods
- Webhook signature verification
- Database operations

### Integration Tests
- Full CLI auth flow
- Stripe checkout creation and completion
- Webhook processing
- Session management

### E2E Tests
- Complete flow from CLI login to billing setup
- Error scenarios (payment failure, timeout)
- Existing user with billing
- Multiple subscription changes

### Manual Testing Checklist
- [ ] CLI login without billing flag
- [ ] CLI login with --setup-billing
- [ ] GitHub auth failure handling
- [ ] Stripe checkout completion
- [ ] Stripe checkout cancellation
- [ ] Webhook processing for all events
- [ ] Session timeout handling
- [ ] Browser compatibility
- [ ] Mobile responsiveness

## Monitoring & Observability

1. **Metrics to Track**
   - CLI auth success/failure rates
   - Stripe checkout conversion rates
   - Webhook processing times
   - Billing status distribution

2. **Logging**
   - All auth attempts
   - Stripe API calls
   - Webhook events
   - Error conditions

3. **Alerts**
   - High auth failure rate
   - Webhook processing failures
   - Stripe API errors
   - Database connection issues

## Documentation Updates

1. **User Documentation**
   - How to set up billing via CLI
   - Troubleshooting auth issues
   - Managing subscriptions
   - Billing FAQ

2. **Developer Documentation**
   - Architecture overview
   - Testing with Stripe test mode
   - Webhook testing with Stripe CLI
   - Environment setup

3. **API Documentation**
   - New billing endpoints
   - Webhook payload formats
   - Error response formats
   - Rate limits

## Success Criteria

- [ ] Seamless flow from `helix login --setup-billing` to active subscription
- [ ] < 30 second end-to-end completion time
- [ ] 99.9% webhook processing success rate
- [ ] Clear error messages at each failure point
- [ ] No regression in existing auth flows
- [ ] Complete audit trail for billing events
- [ ] Support can manage user billing status
- [ ] Users can self-serve subscription changes

## Rollback Plan

1. **Feature Flags**
   - ENABLE_STRIPE_BILLING flag in backend
   - Gradual rollout percentage
   - Quick disable without code changes

2. **Database Rollback**
   - Keep schema additions nullable
   - No modifications to existing fields
   - Prepared rollback migrations

3. **CLI Compatibility**
   - New flag doesn't break old CLI versions
   - Graceful fallback to device flow
   - Clear messaging for unsupported features

## Future Enhancements

1. **Usage-Based Billing**
   - Track query counts and data usage
   - Implement metered billing with Stripe
   - Usage alerts and limits

2. **Team Billing**
   - Organization-level subscriptions
   - Seat management
   - Centralized billing for teams

3. **Self-Service Portal**
   - Update payment methods
   - Download invoices
   - Change subscription tiers
   - Cancel subscription

4. **Advanced Features**
   - Multiple payment methods
   - Annual billing discounts
   - Promotional codes
   - Free trials

## Conclusion

This implementation plan provides a comprehensive approach to integrating Stripe payments into the Helix ecosystem. The design minimizes disruption to existing flows while adding a seamless billing experience initiated from the CLI. By leveraging the existing authentication infrastructure and adding targeted enhancements, we can deliver a production-ready billing system that scales with the platform's growth.
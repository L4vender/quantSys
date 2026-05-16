use quantsys_eventbus::{EventConsumer, EventbusError};

pub async fn drain_once<C>(consumer: &C) -> Result<bool, EventbusError>
where
    C: EventConsumer + Sync,
{
    if let Some(envelope) = consumer.poll().await? {
        consumer.commit(&envelope).await?;
        return Ok(true);
    }
    Ok(false)
}

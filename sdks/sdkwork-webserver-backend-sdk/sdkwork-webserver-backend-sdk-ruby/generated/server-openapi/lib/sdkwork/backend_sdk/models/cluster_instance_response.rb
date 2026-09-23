module Sdkwork
  module BackendSdk
    module Models
      class ClusterInstanceResponse
              attr_accessor :id, :cluster_id, :host_id, :host_name, :name, :role, :environment, :process_pid, :process_started_at, :bind_host, :bind_port, :public_endpoint, :build_version, :status, :health_state, :last_heartbeat_at, :last_online_at, :uptime_seconds, :metrics, :join_mode, :quality_score, :desired_config_revision, :applied_config_revision, :desired_applications_revision, :applied_applications_revision, :sync_status, :routing_enabled, :draining, :ejected, :restart_count, :labels, :routing_weight, :maintenance_note, :probe_failures, :probe_url, :created_at, :updated_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @cluster_id = attributes['clusterId']
                @host_id = attributes['hostId']
                @host_name = attributes['hostName']
                @name = attributes['name']
                @role = attributes['role']
                @environment = attributes['environment']
                @process_pid = attributes['processPid']
                @process_started_at = attributes['processStartedAt']
                @bind_host = attributes['bindHost']
                @bind_port = attributes['bindPort']
                @public_endpoint = attributes['publicEndpoint']
                @build_version = attributes['buildVersion']
                @status = attributes['status']
                @health_state = attributes['healthState']
                @last_heartbeat_at = attributes['lastHeartbeatAt']
                @last_online_at = attributes['lastOnlineAt']
                @uptime_seconds = attributes['uptimeSeconds']
                @metrics = attributes['metrics'].is_a?(Hash) ? attributes['metrics'] : {}
                @join_mode = attributes['joinMode']
                @quality_score = attributes['qualityScore']
                @desired_config_revision = attributes['desiredConfigRevision']
                @applied_config_revision = attributes['appliedConfigRevision']
                @desired_applications_revision = attributes['desiredApplicationsRevision']
                @applied_applications_revision = attributes['appliedApplicationsRevision']
                @sync_status = attributes['syncStatus']
                @routing_enabled = attributes['routingEnabled']
                @draining = attributes['draining']
                @ejected = attributes['ejected']
                @restart_count = attributes['restartCount']
                @labels = attributes['labels'].is_a?(Hash) ? attributes['labels'].transform_values { |item| item } : {}
                @routing_weight = attributes['routingWeight']
                @maintenance_note = attributes['maintenanceNote']
                @probe_failures = attributes['probeFailures']
                @probe_url = attributes['probeUrl']
                @created_at = attributes['createdAt']
                @updated_at = attributes['updatedAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'clusterId' => @cluster_id,
                  'hostId' => @host_id,
                  'hostName' => @host_name,
                  'name' => @name,
                  'role' => @role,
                  'environment' => @environment,
                  'processPid' => @process_pid,
                  'processStartedAt' => @process_started_at,
                  'bindHost' => @bind_host,
                  'bindPort' => @bind_port,
                  'publicEndpoint' => @public_endpoint,
                  'buildVersion' => @build_version,
                  'status' => @status,
                  'healthState' => @health_state,
                  'lastHeartbeatAt' => @last_heartbeat_at,
                  'lastOnlineAt' => @last_online_at,
                  'uptimeSeconds' => @uptime_seconds,
                  'metrics' => @metrics,
                  'joinMode' => @join_mode,
                  'qualityScore' => @quality_score,
                  'desiredConfigRevision' => @desired_config_revision,
                  'appliedConfigRevision' => @applied_config_revision,
                  'desiredApplicationsRevision' => @desired_applications_revision,
                  'appliedApplicationsRevision' => @applied_applications_revision,
                  'syncStatus' => @sync_status,
                  'routingEnabled' => @routing_enabled,
                  'draining' => @draining,
                  'ejected' => @ejected,
                  'restartCount' => @restart_count,
                  'labels' => @labels.is_a?(Hash) ? @labels.transform_values { |item| item } : {},
                  'routingWeight' => @routing_weight,
                  'maintenanceNote' => @maintenance_note,
                  'probeFailures' => @probe_failures,
                  'probeUrl' => @probe_url,
                  'createdAt' => @created_at,
                  'updatedAt' => @updated_at,
                }
              end
            end
    end
  end
end

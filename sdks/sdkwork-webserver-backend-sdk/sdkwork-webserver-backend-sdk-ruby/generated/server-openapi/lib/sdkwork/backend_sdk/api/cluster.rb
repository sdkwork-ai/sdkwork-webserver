require_relative 'base_api'
require_relative '../models/clusters_create_response201'
require_relative '../models/clusters_events_list_response'
require_relative '../models/clusters_hosts_list_response'
require_relative '../models/clusters_hosts_retrieve_response'
require_relative '../models/clusters_hosts_update_response'
require_relative '../models/clusters_instances_cordon_response'
require_relative '../models/clusters_instances_drain_response'
require_relative '../models/clusters_instances_heartbeats_list_response'
require_relative '../models/clusters_instances_list_response'
require_relative '../models/clusters_instances_metrics_list_response'
require_relative '../models/clusters_instances_probe_response'
require_relative '../models/clusters_instances_retrieve_response'
require_relative '../models/clusters_instances_uncordon_response'
require_relative '../models/clusters_instances_undrain_response'
require_relative '../models/clusters_instances_update_response'
require_relative '../models/clusters_list_response'
require_relative '../models/clusters_messages_create_response201'
require_relative '../models/clusters_overview_retrieve_response'
require_relative '../models/clusters_retrieve_response'
require_relative '../models/clusters_sync_response'
require_relative '../models/clusters_update_response'
require_relative '../models/create_cluster_request'
require_relative '../models/enqueue_cluster_peer_messages_request'
require_relative '../models/probe_cluster_instance_request'
require_relative '../models/publish_cluster_sync_request'
require_relative '../models/update_cluster_host_request'
require_relative '../models/update_cluster_instance_request'
require_relative '../models/update_cluster_request'

module Sdkwork
  module BackendSdk
    module Api
      class ClusterApi < BaseApi
          # List Web Server clusters
          def clusters_list(page: nil, page_size: nil)
            path = '/backend/v3/api/clusters'
            query = build_query_string([
              QueryParameterSpec.new('page', page, 'form', true, false, nil),
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ClustersListResponse.from_hash(result) : nil
          end

          # Create a Web Server cluster
          def clusters_create(idempotency_key, body: nil)
            path = '/backend/v3/api/clusters'
            payload = body.respond_to?(:to_hash) ? body.to_hash : body
            request_headers = build_request_headers(
              {
                'Idempotency-Key' => HeaderParameterSpec.new(idempotency_key, 'simple', false, nil),
              },
              {}
            )
            options = {}
            options[:headers] = request_headers unless request_headers.empty?
            options[:json] = payload unless payload.nil?
            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::ClustersCreateResponse201.from_hash(result) : nil
          end

          # Retrieve a Web Server cluster
          def clusters_retrieve(cluster_id)
            path = interpolate_path('/backend/v3/api/clusters/{clusterId}', clusterId: serialize_path_parameter(cluster_id, PathParameterSpec.new('clusterId', 'simple', false)))
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ClustersRetrieveResponse.from_hash(result) : nil
          end

          # Update a Web Server cluster
          def clusters_update(cluster_id, idempotency_key, body: nil)
            path = interpolate_path('/backend/v3/api/clusters/{clusterId}', clusterId: serialize_path_parameter(cluster_id, PathParameterSpec.new('clusterId', 'simple', false)))
            payload = body.respond_to?(:to_hash) ? body.to_hash : body
            request_headers = build_request_headers(
              {
                'Idempotency-Key' => HeaderParameterSpec.new(idempotency_key, 'simple', false, nil),
              },
              {}
            )
            options = {}
            options[:headers] = request_headers unless request_headers.empty?
            options[:json] = payload unless payload.nil?
            result = @client.request('PATCH', path, **options)
            result.is_a?(Hash) ? Models::ClustersUpdateResponse.from_hash(result) : nil
          end

          # Delete an empty Web Server cluster
          def clusters_delete(cluster_id, idempotency_key)
            path = interpolate_path('/backend/v3/api/clusters/{clusterId}', clusterId: serialize_path_parameter(cluster_id, PathParameterSpec.new('clusterId', 'simple', false)))
            request_headers = build_request_headers(
              {
                'Idempotency-Key' => HeaderParameterSpec.new(idempotency_key, 'simple', false, nil),
              },
              {}
            )
            options = {}
            options[:headers] = request_headers unless request_headers.empty?
            @client.request('DELETE', path, **options)
            nil
          end

          # Publish a desired-state revision to every instance of the cluster
          def clusters_sync(cluster_id, body: nil)
            path = interpolate_path('/backend/v3/api/clusters/{clusterId}/sync', clusterId: serialize_path_parameter(cluster_id, PathParameterSpec.new('clusterId', 'simple', false)))
            payload = body.respond_to?(:to_hash) ? body.to_hash : body
            options = {}
            options[:json] = payload unless payload.nil?
            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::ClustersSyncResponse.from_hash(result) : nil
          end

          # List cluster hosts with system and network identity
          def clusters_hosts_list(page_size: nil, cursor: nil, cluster_id: nil, status: nil)
            path = '/backend/v3/api/clusters/hosts'
            query = build_query_string([
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
              QueryParameterSpec.new('cursor', cursor, 'form', true, false, nil),
              QueryParameterSpec.new('cluster_id', cluster_id, 'form', true, false, nil),
              QueryParameterSpec.new('status', status, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ClustersHostsListResponse.from_hash(result) : nil
          end

          # Retrieve a cluster host
          def clusters_hosts_retrieve(host_id)
            path = interpolate_path('/backend/v3/api/clusters/hosts/{hostId}', hostId: serialize_path_parameter(host_id, PathParameterSpec.new('hostId', 'simple', false)))
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ClustersHostsRetrieveResponse.from_hash(result) : nil
          end

          # Rename a host or reassign it to another cluster
          def clusters_hosts_update(host_id, idempotency_key, body: nil)
            path = interpolate_path('/backend/v3/api/clusters/hosts/{hostId}', hostId: serialize_path_parameter(host_id, PathParameterSpec.new('hostId', 'simple', false)))
            payload = body.respond_to?(:to_hash) ? body.to_hash : body
            request_headers = build_request_headers(
              {
                'Idempotency-Key' => HeaderParameterSpec.new(idempotency_key, 'simple', false, nil),
              },
              {}
            )
            options = {}
            options[:headers] = request_headers unless request_headers.empty?
            options[:json] = payload unless payload.nil?
            result = @client.request('PATCH', path, **options)
            result.is_a?(Hash) ? Models::ClustersHostsUpdateResponse.from_hash(result) : nil
          end

          # Remove an instance-free host from the cluster inventory
          def clusters_hosts_delete(host_id, idempotency_key)
            path = interpolate_path('/backend/v3/api/clusters/hosts/{hostId}', hostId: serialize_path_parameter(host_id, PathParameterSpec.new('hostId', 'simple', false)))
            request_headers = build_request_headers(
              {
                'Idempotency-Key' => HeaderParameterSpec.new(idempotency_key, 'simple', false, nil),
              },
              {}
            )
            options = {}
            options[:headers] = request_headers unless request_headers.empty?
            @client.request('DELETE', path, **options)
            nil
          end

          # List webserver process instances with liveness state
          def clusters_instances_list(page_size: nil, cursor: nil, cluster_id: nil, host_id: nil, status: nil, health_state: nil)
            path = '/backend/v3/api/clusters/instances'
            query = build_query_string([
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
              QueryParameterSpec.new('cursor', cursor, 'form', true, false, nil),
              QueryParameterSpec.new('cluster_id', cluster_id, 'form', true, false, nil),
              QueryParameterSpec.new('host_id', host_id, 'form', true, false, nil),
              QueryParameterSpec.new('status', status, 'form', true, false, nil),
              QueryParameterSpec.new('health_state', health_state, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ClustersInstancesListResponse.from_hash(result) : nil
          end

          # Retrieve a webserver process instance
          def clusters_instances_retrieve(instance_id)
            path = interpolate_path('/backend/v3/api/clusters/instances/{instanceId}', instanceId: serialize_path_parameter(instance_id, PathParameterSpec.new('instanceId', 'simple', false)))
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ClustersInstancesRetrieveResponse.from_hash(result) : nil
          end

          # Update an instance display name, status, or advertised endpoint
          def clusters_instances_update(instance_id, idempotency_key, body: nil)
            path = interpolate_path('/backend/v3/api/clusters/instances/{instanceId}', instanceId: serialize_path_parameter(instance_id, PathParameterSpec.new('instanceId', 'simple', false)))
            payload = body.respond_to?(:to_hash) ? body.to_hash : body
            request_headers = build_request_headers(
              {
                'Idempotency-Key' => HeaderParameterSpec.new(idempotency_key, 'simple', false, nil),
              },
              {}
            )
            options = {}
            options[:headers] = request_headers unless request_headers.empty?
            options[:json] = payload unless payload.nil?
            result = @client.request('PATCH', path, **options)
            result.is_a?(Hash) ? Models::ClustersInstancesUpdateResponse.from_hash(result) : nil
          end

          # Unregister a webserver process instance
          def clusters_instances_delete(instance_id, idempotency_key)
            path = interpolate_path('/backend/v3/api/clusters/instances/{instanceId}', instanceId: serialize_path_parameter(instance_id, PathParameterSpec.new('instanceId', 'simple', false)))
            request_headers = build_request_headers(
              {
                'Idempotency-Key' => HeaderParameterSpec.new(idempotency_key, 'simple', false, nil),
              },
              {}
            )
            options = {}
            options[:headers] = request_headers unless request_headers.empty?
            @client.request('DELETE', path, **options)
            nil
          end

          # List cluster lifecycle events
          def clusters_events_list(page_size: nil, cursor: nil, cluster_id: nil, severity: nil)
            path = '/backend/v3/api/clusters/events'
            query = build_query_string([
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
              QueryParameterSpec.new('cursor', cursor, 'form', true, false, nil),
              QueryParameterSpec.new('cluster_id', cluster_id, 'form', true, false, nil),
              QueryParameterSpec.new('severity', severity, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ClustersEventsListResponse.from_hash(result) : nil
          end

          # Retrieve the cluster health overview for status polling
          def clusters_overview_retrieve()
            path = '/backend/v3/api/clusters/overview'
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ClustersOverviewRetrieveResponse.from_hash(result) : nil
          end

          # List one instance's stored heartbeat samples
          def clusters_instances_heartbeats_list(instance_id, page_size: nil, cursor: nil)
            path = interpolate_path('/backend/v3/api/clusters/instances/{instanceId}/heartbeats', instanceId: serialize_path_parameter(instance_id, PathParameterSpec.new('instanceId', 'simple', false)))
            query = build_query_string([
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
              QueryParameterSpec.new('cursor', cursor, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ClustersInstancesHeartbeatsListResponse.from_hash(result) : nil
          end

          # List one instance's heartbeat metric samples for trend charts
          def clusters_instances_metrics_list(instance_id, page_size: nil, cursor: nil)
            path = interpolate_path('/backend/v3/api/clusters/instances/{instanceId}/metrics/history', instanceId: serialize_path_parameter(instance_id, PathParameterSpec.new('instanceId', 'simple', false)))
            query = build_query_string([
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
              QueryParameterSpec.new('cursor', cursor, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ClustersInstancesMetricsListResponse.from_hash(result) : nil
          end

          # Probe one instance's connectivity and record the outcome
          def clusters_instances_probe(instance_id, body: nil)
            path = interpolate_path('/backend/v3/api/clusters/instances/{instanceId}/probe', instanceId: serialize_path_parameter(instance_id, PathParameterSpec.new('instanceId', 'simple', false)))
            payload = body.respond_to?(:to_hash) ? body.to_hash : body
            options = {}
            options[:json] = payload unless payload.nil?
            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::ClustersInstancesProbeResponse.from_hash(result) : nil
          end

          # Gracefully drain one instance out of routing
          def clusters_instances_drain(instance_id)
            path = interpolate_path('/backend/v3/api/clusters/instances/{instanceId}/drain', instanceId: serialize_path_parameter(instance_id, PathParameterSpec.new('instanceId', 'simple', false)))
            options = {}

            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::ClustersInstancesDrainResponse.from_hash(result) : nil
          end

          # Clear the drain flag and restore routing participation
          def clusters_instances_undrain(instance_id)
            path = interpolate_path('/backend/v3/api/clusters/instances/{instanceId}/undrain', instanceId: serialize_path_parameter(instance_id, PathParameterSpec.new('instanceId', 'simple', false)))
            options = {}

            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::ClustersInstancesUndrainResponse.from_hash(result) : nil
          end

          # Cordon one instance out of routing without draining it
          def clusters_instances_cordon(instance_id)
            path = interpolate_path('/backend/v3/api/clusters/instances/{instanceId}/cordon', instanceId: serialize_path_parameter(instance_id, PathParameterSpec.new('instanceId', 'simple', false)))
            options = {}

            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::ClustersInstancesCordonResponse.from_hash(result) : nil
          end

          # Uncordon one instance back into routing
          def clusters_instances_uncordon(instance_id)
            path = interpolate_path('/backend/v3/api/clusters/instances/{instanceId}/uncordon', instanceId: serialize_path_parameter(instance_id, PathParameterSpec.new('instanceId', 'simple', false)))
            options = {}

            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::ClustersInstancesUncordonResponse.from_hash(result) : nil
          end

          # Enqueue a peer message to one instance or broadcast to online members
          def clusters_messages_create(idempotency_key, body: nil)
            path = '/backend/v3/api/clusters/messages'
            payload = body.respond_to?(:to_hash) ? body.to_hash : body
            request_headers = build_request_headers(
              {
                'Idempotency-Key' => HeaderParameterSpec.new(idempotency_key, 'simple', false, nil),
              },
              {}
            )
            options = {}
            options[:headers] = request_headers unless request_headers.empty?
            options[:json] = payload unless payload.nil?
            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::ClustersMessagesCreateResponse201.from_hash(result) : nil
          end

        private

        def build_request_headers(headers = {}, cookies = {})
          request_headers = {}
          headers.each do |name, parameter|
            serialized = serialize_parameter_value(parameter)
            request_headers[name.to_s] = serialized unless serialized.nil?
          end

          cookie_header = build_cookie_header(cookies)
          unless cookie_header.empty?
            request_headers['Cookie'] =
              request_headers.key?('Cookie') && !request_headers['Cookie'].empty? ? "#{request_headers['Cookie']}; #{cookie_header}" : cookie_header
          end

          request_headers
        end

        def build_cookie_header(cookies = {})
          cookies.filter_map do |name, parameter|
            serialized = serialize_parameter_value(parameter)
            next if serialized.nil?

            "#{CGI.escape(name.to_s)}=#{CGI.escape(serialized)}"
          end.join('; ')
        end

        def serialize_parameter_value(parameter)
          value = parameter&.value
          return nil if value.nil?
          return JSON.generate(value) if parameter.content_type && !parameter.content_type.empty?
          return value.compact.map(&:to_s).join(',') if value.is_a?(Array)
          if value.is_a?(Hash)
            serialized = []
            value.each do |key, item|
              next if item.nil?
              if parameter.explode
                serialized << "#{key}=#{item}"
              else
                serialized << key.to_s
                serialized << item.to_s
              end
            end
            return serialized.join(',')
          end
          return value.iso8601 if value.respond_to?(:iso8601)

          value.to_s
        end
      end
    end
  end
end

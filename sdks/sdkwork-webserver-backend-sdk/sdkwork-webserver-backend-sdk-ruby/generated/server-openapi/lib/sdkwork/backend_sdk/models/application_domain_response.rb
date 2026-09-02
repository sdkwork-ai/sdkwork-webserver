module Sdkwork
  module BackendSdk
    module Models
      class ApplicationDomainResponse
              attr_accessor :id, :hostname, :root_domain_id, :record_name, :application_id, :application_name, :certificate_count, :is_primary, :is_verified, :ssl_enabled, :ssl_provider, :status, :latest_deployment, :created_at, :updated_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @hostname = attributes['hostname']
                @root_domain_id = attributes['rootDomainId']
                @record_name = attributes['recordName']
                @application_id = attributes['applicationId']
                @application_name = attributes['applicationName']
                @certificate_count = attributes['certificateCount']
                @is_primary = attributes['isPrimary']
                @is_verified = attributes['isVerified']
                @ssl_enabled = attributes['sslEnabled']
                @ssl_provider = attributes['sslProvider']
                @status = attributes['status']
                @latest_deployment = attributes['latestDeployment'].is_a?(Hash) ? DomainDeploymentResponse.from_hash(attributes['latestDeployment']) : nil
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
                  'hostname' => @hostname,
                  'rootDomainId' => @root_domain_id,
                  'recordName' => @record_name,
                  'applicationId' => @application_id,
                  'applicationName' => @application_name,
                  'certificateCount' => @certificate_count,
                  'isPrimary' => @is_primary,
                  'isVerified' => @is_verified,
                  'sslEnabled' => @ssl_enabled,
                  'sslProvider' => @ssl_provider,
                  'status' => @status,
                  'latestDeployment' => @latest_deployment&.to_hash,
                  'createdAt' => @created_at,
                  'updatedAt' => @updated_at,
                }
              end
            end
    end
  end
end

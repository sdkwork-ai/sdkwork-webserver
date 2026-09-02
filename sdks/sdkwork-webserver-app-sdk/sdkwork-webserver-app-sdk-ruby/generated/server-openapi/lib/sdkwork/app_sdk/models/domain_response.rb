module Sdkwork
  module AppSdk
    module Models
      class DomainResponse
              attr_accessor :id, :hostname, :application_id, :application_name, :certificate_count, :is_primary, :is_verified, :ssl_enabled, :ssl_provider, :status, :created_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @hostname = attributes['hostname']
                @application_id = attributes['applicationId']
                @application_name = attributes['applicationName']
                @certificate_count = attributes['certificateCount']
                @is_primary = attributes['isPrimary']
                @is_verified = attributes['isVerified']
                @ssl_enabled = attributes['sslEnabled']
                @ssl_provider = attributes['sslProvider']
                @status = attributes['status']
                @created_at = attributes['createdAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'hostname' => @hostname,
                  'applicationId' => @application_id,
                  'applicationName' => @application_name,
                  'certificateCount' => @certificate_count,
                  'isPrimary' => @is_primary,
                  'isVerified' => @is_verified,
                  'sslEnabled' => @ssl_enabled,
                  'sslProvider' => @ssl_provider,
                  'status' => @status,
                  'createdAt' => @created_at,
                }
              end
            end
    end
  end
end

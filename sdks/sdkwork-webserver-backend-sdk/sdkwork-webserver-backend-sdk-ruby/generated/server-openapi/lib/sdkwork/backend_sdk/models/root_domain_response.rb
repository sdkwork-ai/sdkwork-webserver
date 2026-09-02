module Sdkwork
  module BackendSdk
    module Models
      class RootDomainResponse
              attr_accessor :id, :hostname, :status, :subdomain_count, :bound_subdomain_count, :verified_subdomain_count, :https_subdomain_count, :active_deployment_count, :created_at, :updated_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @hostname = attributes['hostname']
                @status = attributes['status']
                @subdomain_count = attributes['subdomainCount']
                @bound_subdomain_count = attributes['boundSubdomainCount']
                @verified_subdomain_count = attributes['verifiedSubdomainCount']
                @https_subdomain_count = attributes['httpsSubdomainCount']
                @active_deployment_count = attributes['activeDeploymentCount']
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
                  'status' => @status,
                  'subdomainCount' => @subdomain_count,
                  'boundSubdomainCount' => @bound_subdomain_count,
                  'verifiedSubdomainCount' => @verified_subdomain_count,
                  'httpsSubdomainCount' => @https_subdomain_count,
                  'activeDeploymentCount' => @active_deployment_count,
                  'createdAt' => @created_at,
                  'updatedAt' => @updated_at,
                }
              end
            end
    end
  end
end

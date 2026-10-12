-- Same shape as the user prod will get: SELECT on only the tables the dashboard reads.
CREATE USER 'dashboard_ro'@'%' IDENTIFIED BY 'dev';
GRANT SELECT ON gitea.repository TO 'dashboard_ro'@'%';
GRANT SELECT ON gitea.topic      TO 'dashboard_ro'@'%';
GRANT SELECT ON gitea.repo_topic TO 'dashboard_ro'@'%';
GRANT SELECT ON gitea.`release` TO 'dashboard_ro'@'%';
GRANT SELECT ON gitea.attachment TO 'dashboard_ro'@'%';
FLUSH PRIVILEGES;

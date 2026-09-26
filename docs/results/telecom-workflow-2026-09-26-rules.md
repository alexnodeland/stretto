# The compiled telecom workflow, as rules

The workflow of [a workflow compiled once, run with no model](telecom-workflow-2026-09-26.md), as `scripts/telecom_workflow.py --show` writes it: per site (the call that just returned, busiest first), the tree's questions down to its chosen depth, and at each leaf the call it makes, with how many of the training cases there made it. It is 376 rules at 39 sites; 40 of them are sure (one call in at least 95% of at least ten cases), covering 2,056 of the 12,059 training decisions. Features name a tool and a field of its last result (`run_speed_test:speed test failed=no connection.`), a tool called (`called check_network_status`), a write made (`made …`), or a ticket word (`ticket: abroad`).

### After `get_details_by_id` (1764 decisions, 4 deep)

- if `get_details_by_id:status=active`:
  - if `get_details_by_id:roaming_enabled=false`:
    - if `called check_network_status`:
      - if `called can_send_mms`:
        - then **get_details_by_id{"id": "L1002"}** (54 of 102)
      - else:
        - then **get_details_by_id{"id": "L1002"}** (50 of 98)
    - else:
      - if `ticket: abroad`:
        - then **get_details_by_id{"id": "L1002"}** (305 of 539)
      - else:
        - then **get_details_by_id{"id": "L1002"}** (164 of 173)
  - else:
    - if `called check_network_status`:
      - if `run_speed_test:speed test failed=no connection.`:
        - then **get_details_by_id{"id": "P1002"}** (14 of 28)
      - else:
        - then **run_speed_test** (5 of 32)
    - else:
      - if `ticket: browse`:
        - then **get_details_by_id{"id": "P1002"}** (97 of 153)
      - else:
        - then **get_details_by_id{"id": "D1002"}** (24 of 86)
- else:
  - if `ticket: all`:
    - if `called get_bills_for_customer`:
      - if `made send_payment_request{"bill_id": "B1234321", "customer_id": "C1001"}`:
        - then **resume_line{"customer_id": "C1001", "line_id": "L1002"}** (45 of 47)
      - else:
        - then **check_status_bar** (24 of 49)
    - else:
      - if `get_details_by_id:roaming_enabled=true`:
        - then **get_bills_for_customer{"customer_id": "C1001"}** (24 of 51)
      - else:
        - then **get_bills_for_customer{"customer_id": "C1001"}** (40 of 53)
  - else:
    - if `called check_network_status`:
      - if `run_speed_test:speed test failed=no connection.`:
        - then **refuel_data{"customer_id": "C1001", "gb_amount": 2, "line_id": "L1002"}** (28 of 64)
      - else:
        - then **set_network_mode_preference{"mode": "4g_5g_preferred"}** (5 of 38)
    - else:
      - if `ticket: abroad`:
        - then **enable_roaming{"customer_id": "C1001", "line_id": "L1002"}** (65 of 196)
      - else:
        - then **check_status_bar** (23 of 55)

### After `run_speed_test` (1236 decisions, 6 deep)

- if `run_speed_test:speed test failed=no connection.`:
  - if `called check_network_status`:
    - if `called get_details_by_id`:
      - if `called toggle_data_saver_mode`:
        - if `called refuel_data`:
          - if `called reboot_device`:
            - then **set_network_mode_preference{"mode": "4g_only"}** (2 of 5)
          - else:
            - then **check_apn_settings** (2 of 6)
        - else:
          - if `get_data_usage:data_used_gb=15.1`:
            - then **check_apn_settings** (3 of 7)
          - else:
            - then **get_data_usage{"customer_id": "C1001", "line_id": "L1002"}** (9 of 24)
      - else:
        - if `made enable_roaming{"customer_id": "C1001", "line_id": "L1002"}`:
          - if `check_network_status:mobile data enabled=no`:
            - then **get_data_usage{"customer_id": "C1001", "line_id": "L1002"}** (13 of 31)
          - else:
            - then **get_data_usage{"customer_id": "C1001", "line_id": "L1002"}** (9 of 25)
        - else:
          - if `get_details_by_id:roaming_enabled=true`:
            - then **get_data_usage{"customer_id": "C1001", "line_id": "L1002"}** (7 of 28)
          - else:
            - then **enable_roaming{"customer_id": "C1001", "line_id": "L1002"}** (19 of 40)
    - else:
      - if `get_customer_by_phone:account_status=active`:
        - if `reseat_sim_card:status bar=📶⁴ excellent`:
          - then **get_details_by_id{"id": "L1001"}** (3 of 5)
        - else:
          - if `ticket: abroad`:
            - then **get_details_by_id{"id": "L1001"}** (14 of 20)
          - else:
            - then **get_details_by_id{"id": "L1001"}** (6 of 9)
      - else:
        - if `toggle_airplane_mode:status bar=📶¹ poor`:
          - then **get_customer_by_phone{"phone_number": "555-123-2002"}** (2 of 5)
        - else:
          - if `toggle_roaming:status bar=📱 data enabled`:
            - then **get_customer_by_phone{"phone_number": "555-123-2002"}** (12 of 12)
          - else:
            - then **get_customer_by_phone{"phone_number": "555-123-2002"}** (5 of 7)
  - else:
    - if `called check_status_bar`:
      - if `called check_sim_status`:
        - if `called get_data_usage`:
          - if `reboot_device:status bar=📶⁴ excellent`:
            - then **check_network_status** (10 of 13)
          - else:
            - then **check_network_status** (8 of 12)
        - else:
          - if `reboot_device:status bar=📱 data enabled`:
            - then **get_data_usage{"customer_id": "C1001", "line_id": "L1002"}** (5 of 11)
          - else:
            - then **check_network_status** (3 of 10)
      - else:
        - if `called toggle_airplane_mode`:
          - if `get_details_by_id:status=active`:
            - then **check_network_status** (4 of 10)
          - else:
            - then **check_sim_status** (39 of 51)
        - else:
          - if `ticket: abroad`:
            - then **check_network_status** (21 of 36)
          - else:
            - then **check_network_status** (9 of 15)
    - else:
      - if `called toggle_roaming`:
        - if `called check_apn_settings`:
          - if `toggle_roaming:status bar=📱 data enabled`:
            - then **get_data_usage{"customer_id": "C1001", "line_id": "L1002"}** (5 of 8)
          - else:
            - then **get_data_usage{"customer_id": "C1001", "line_id": "L1002"}** (5 of 9)
        - else:
          - if `toggle_data:status bar=📶¹ poor`:
            - then **get_data_usage{"customer_id": "C1001", "line_id": "L1002"}** (4 of 11)
          - else:
            - then **check_sim_status** (11 of 14)
      - else:
        - if `get_details_by_id:name=premium plan`:
          - if `called enable_roaming`:
            - then **check_status_bar** (15 of 15)
          - else:
            - then **check_status_bar** (28 of 31)
        - else:
          - then **check_network_status** (3 of 5)
- else:
  - if `ticket: an`:
    - if `called set_network_mode_preference`:
      - if `ticket: abroad`:
        - if `get_details_by_id:roaming_enabled=false`:
          - if `check_network_status:mobile data enabled=yes`:
            - then **can_send_mms** (7 of 8)
          - else:
            - then **can_send_mms** (3 of 7)
        - else:
          - then **can_send_mms** (34 of 34)
      - else:
        - then **can_send_mms** (3 of 8)
    - else:
      - if `toggle_data:status bar=📶¹ poor`:
        - if `called check_status_bar`:
          - if `called refuel_data`:
            - then **check_data_restriction_status** (13 of 16)
          - else:
            - then **check_network_mode_preference** (4 of 5)
        - else:
          - then **set_network_mode_preference{"mode": "4g_5g_preferred"}** (2 of 5)
      - else:
        - if `check_network_status:cellular connection=no_service`:
          - then **check_installed_apps** (2 of 9)
        - else:
          - if `get_details_by_id:roaming_enabled=false`:
            - then **can_send_mms** (4 of 9)
          - else:
            - then **can_send_mms** (19 of 21)
  - else:
    - if `set_network_mode_preference:status bar=📶⁴ excellent`:
      - if `set_network_mode_preference:status bar=🔒 vpn connected`:
        - if `called disconnect_vpn`:
          - then **stop** (117 of 117)
        - else:
          - if `called toggle_data_saver_mode`:
            - then **check_vpn_status** (42 of 51)
          - else:
            - then **check_vpn_status** (14 of 41)
      - else:
        - if `set_network_mode_preference:status bar=🔽 data saver`:
          - if `called check_data_restriction_status`:
            - then **stop** (7 of 7)
          - else:
            - then **stop** (4 of 7)
        - else:
          - if `toggle_roaming:status bar=🔽 data saver`:
            - then **stop** (9 of 10)
          - else:
            - then **stop** (145 of 145)
    - else:
      - if `check_network_status:cellular signal=poor`:
        - if `called toggle_data_saver_mode`:
          - if `called check_data_restriction_status`:
            - then **check_network_mode_preference** (15 of 16)
          - else:
            - then **set_network_mode_preference{"mode": "4g_5g_preferred"}** (4 of 5)
        - else:
          - if `get_details_by_id:status=active`:
            - then **check_data_restriction_status** (8 of 20)
          - else:
            - then **check_data_restriction_status** (21 of 41)
      - else:
        - if `toggle_airplane_mode:status bar=📶¹ poor`:
          - if `toggle_data:status bar=🔽 data saver`:
            - then **toggle_data_saver_mode** (5 of 11)
          - else:
            - then **check_data_restriction_status** (17 of 22)
        - else:
          - if `called toggle_data_saver_mode`:
            - then **stop** (25 of 37)
          - else:
            - then **stop** (76 of 109)

### After `get_customer_by_phone` (787 decisions, 1 deep)

- if `called get_details_by_id`:
  - then **get_details_by_id{"id": "L1002"}** (31 of 67)
- else:
  - then **get_details_by_id{"id": "L1001"}** (538 of 720)

### After `start` (730 decisions, 0 deep)

- then **get_customer_by_phone{"phone_number": "555-123-2002"}** (690 of 730)

### After `can_send_mms` (652 decisions, 5 deep)

- if `can_send_mms:says your messaging app can send mms messages.`:
  - if `called reboot_device`:
    - then **stop** (100 of 100)
  - else:
    - if `called run_speed_test`:
      - then **stop** (44 of 44)
    - else:
      - if `called check_installed_apps`:
        - then **stop** (7 of 7)
      - else:
        - if `reseat_sim_card:status bar=📶¹ poor`:
          - then **reboot_device** (5 of 7)
        - else:
          - then **stop** (15 of 19)
- else:
  - if `called check_network_status`:
    - if `called check_wifi_calling_status`:
      - if `called run_speed_test`:
        - if `called check_app_permissions`:
          - then **check_apn_settings** (11 of 22)
        - else:
          - then **check_installed_apps** (14 of 34)
      - else:
        - if `called check_app_permissions`:
          - then **check_apn_settings** (11 of 34)
        - else:
          - then **check_app_permissions{"app_name": "messaging"}** (19 of 36)
    - else:
      - if `called check_apn_settings`:
        - if `called reseat_sim_card`:
          - then **check_wifi_calling_status** (23 of 35)
        - else:
          - then **check_wifi_calling_status** (20 of 38)
      - else:
        - if `check_sim_status:says no sim card detected in the phone.`:
          - then **check_apn_settings** (18 of 39)
        - else:
          - then **check_apn_settings** (30 of 69)
  - else:
    - if `check_status_bar:error=False`:
      - if `called check_wifi_calling_status`:
        - if `called check_installed_apps`:
          - then **check_apn_settings** (4 of 5)
        - else:
          - then **check_installed_apps** (4 of 5)
      - else:
        - if `ticket: abroad`:
          - then **check_apn_settings** (6 of 12)
        - else:
          - then **check_wifi_calling_status** (8 of 14)
    - else:
      - if `called toggle_roaming`:
        - if `toggle_roaming:status bar=📶⁴ excellent`:
          - then **check_apn_settings** (3 of 7)
        - else:
          - then **toggle_roaming** (3 of 7)
      - else:
        - if `called check_status_bar`:
          - then **check_network_status** (15 of 15)
        - else:
          - then **check_status_bar** (81 of 103)

### After `check_status_bar` (609 decisions, 6 deep)

- if `check_status_bar:status bar=✈️ airplane mode`:
  - if `called toggle_airplane_mode`:
    - then **disconnect_vpn** (12 of 12)
  - else:
    - if `get_details_by_id:activated=true`:
      - if `ticket: abroad`:
        - if `check_status_bar:status bar=🔒 vpn connected`:
          - then **toggle_airplane_mode** (7 of 8)
        - else:
          - then **toggle_airplane_mode** (7 of 7)
      - else:
        - then **check_network_status** (5 of 6)
    - else:
      - if `get_details_by_id:roaming_enabled=false`:
        - if `ticket: browse`:
          - if `check_status_bar:status bar=🔒 vpn connected`:
            - then **toggle_airplane_mode** (7 of 9)
          - else:
            - then **toggle_airplane_mode** (12 of 14)
        - else:
          - if `get_details_by_id:status=active`:
            - then **toggle_airplane_mode** (7 of 7)
          - else:
            - then **toggle_airplane_mode** (4 of 5)
      - else:
        - if `get_details_by_id:status=suspended`:
          - then **toggle_airplane_mode** (13 of 14)
        - else:
          - if `get_details_by_id:roaming_enabled=true`:
            - then **toggle_airplane_mode** (26 of 29)
          - else:
            - then **toggle_airplane_mode** (196 of 200)
- else:
  - if `check_status_bar:status bar=📱 data enabled`:
    - if `check_status_bar:status bar=📶¹ poor`:
      - if `check_status_bar:status bar=🔽 data saver`:
        - if `called toggle_data_saver_mode`:
          - then **set_network_mode_preference{"mode": "4g_5g_preferred"}** (4 of 5)
        - else:
          - if `called get_data_usage`:
            - then **toggle_data_saver_mode** (5 of 8)
          - else:
            - then **run_speed_test** (2 of 5)
      - else:
        - if `ticket: an`:
          - if `called check_sim_status`:
            - then **check_network_mode_preference** (2 of 6)
          - else:
            - then **set_network_mode_preference{"mode": "4g_5g_preferred"}** (2 of 5)
        - else:
          - if `get_details_by_id:status=active`:
            - then **check_network_status** (5 of 11)
          - else:
            - then **check_network_status** (16 of 31)
    - else:
      - if `ticket: all`:
        - if `called check_payment_request`:
          - then **stop** (8 of 8)
        - else:
          - then **stop** (7 of 9)
      - else:
        - if `called check_network_status`:
          - if `get_details_by_id:status=active`:
            - then **enable_roaming{"customer_id": "C1001", "line_id": "L1002"}** (2 of 5)
          - else:
            - then **run_speed_test** (3 of 6)
        - else:
          - if `check_status_bar:status bar=🔽 data saver`:
            - then **check_network_status** (6 of 8)
          - else:
            - then **run_speed_test** (18 of 23)
  - else:
    - if `check_status_bar:status bar=📵 no signal`:
      - if `called check_sim_status`:
        - if `called check_network_status`:
          - if `get_details_by_id:roaming_enabled=true`:
            - then **reset_apn_settings** (3 of 5)
          - else:
            - then **reset_apn_settings** (6 of 8)
        - else:
          - if `get_details_by_id:len(line_items)=1`:
            - then **reset_apn_settings** (5 of 6)
          - else:
            - then **check_network_status** (6 of 7)
      - else:
        - if `called get_details_by_id`:
          - if `reboot_device:status bar=✈️ airplane mode`:
            - then **check_sim_status** (10 of 12)
          - else:
            - then **check_network_status** (27 of 44)
        - else:
          - if `ticket: abroad`:
            - then **check_network_status** (8 of 9)
          - else:
            - then **check_network_status** (11 of 11)
    - else:
      - if `called toggle_data`:
        - then **set_network_mode_preference{"mode": "4g_5g_preferred"}** (2 of 5)
      - else:
        - if `get_details_by_id:name=premium plan`:
          - if `check_status_bar:status bar=🔒 vpn connected`:
            - then **check_network_status** (3 of 7)
          - else:
            - then **toggle_data** (17 of 17)
        - else:
          - if `called check_sim_status`:
            - then **check_network_status** (8 of 8)
          - else:
            - then **toggle_data** (13 of 29)

### After `check_network_status` (609 decisions, 6 deep)

- if `check_network_status:cellular connection=connected`:
  - if `check_network_status:mobile data enabled=no`:
    - if `called toggle_data`:
      - if `get_details_by_id:activated=true`:
        - then **toggle_roaming** (4 of 5)
      - else:
        - then **toggle_roaming** (15 of 15)
    - else:
      - if `get_details_by_id:roaming_enabled=false`:
        - if `check_network_status:data roaming enabled=no`:
          - if `called check_status_bar`:
            - then **toggle_roaming** (3 of 7)
          - else:
            - then **toggle_data** (2 of 7)
        - else:
          - then **toggle_data** (3 of 5)
      - else:
        - if `called can_send_mms`:
          - if `check_status_bar:status bar=✈️ airplane mode`:
            - then **toggle_data** (12 of 15)
          - else:
            - then **toggle_data** (13 of 13)
        - else:
          - if `called get_details_by_id`:
            - then **toggle_data** (18 of 19)
          - else:
            - then **toggle_data** (10 of 16)
  - else:
    - if `check_network_status:data roaming enabled=no`:
      - if `ticket: abroad`:
        - if `get_details_by_id:roaming_enabled=false`:
          - if `called enable_roaming`:
            - then **toggle_roaming** (11 of 14)
          - else:
            - then **toggle_roaming** (5 of 13)
        - else:
          - if `called get_details_by_id`:
            - then **toggle_roaming** (112 of 115)
          - else:
            - then **toggle_roaming** (9 of 15)
      - else:
        - if `get_details_by_id:status=active`:
          - if `check_status_bar:status bar=📱 data enabled`:
            - then **get_data_usage{"customer_id": "C1001", "line_id": "L1002"}** (4 of 8)
          - else:
            - then **set_network_mode_preference{"mode": "4g_5g_preferred"}** (3 of 8)
        - else:
          - if `called run_speed_test`:
            - then **get_details_by_id{"id": "L1001"}** (4 of 10)
          - else:
            - then **run_speed_test** (6 of 11)
    - else:
      - if `called can_send_mms`:
        - if `reseat_sim_card:status bar=📶¹ poor`:
          - if `called check_data_restriction_status`:
            - then **check_network_mode_preference** (9 of 9)
          - else:
            - then **check_network_mode_preference** (3 of 6)
        - else:
          - if `called get_customer_by_phone`:
            - then **can_send_mms** (4 of 16)
          - else:
            - then **get_customer_by_phone{"phone_number": "555-123-2002"}** (5 of 5)
      - else:
        - if `called get_details_by_id`:
          - if `called enable_roaming`:
            - then **run_speed_test** (2 of 7)
          - else:
            - then **enable_roaming{"customer_id": "C1001", "line_id": "L1002"}** (17 of 25)
        - else:
          - then **get_details_by_id{"id": "L1001"}** (3 of 9)
- else:
  - if `check_network_status:airplane mode=off`:
    - if `check_network_status:sim card status=missing`:
      - if `called get_customer_by_phone`:
        - if `get_details_by_id:status=active`:
          - if `called can_send_mms`:
            - then **reseat_sim_card** (7 of 9)
          - else:
            - then **check_sim_status** (12 of 15)
        - else:
          - if `ticket: all`:
            - then **reseat_sim_card** (37 of 39)
          - else:
            - then **reseat_sim_card** (16 of 24)
      - else:
        - then **check_sim_status** (11 of 11)
    - else:
      - if `check_network_status:sim card status=active`:
        - then **reboot_device** (2 of 8)
      - else:
        - if `called reboot_device`:
          - then **check_sim_status** (5 of 5)
        - else:
          - if `get_details_by_id:roaming_enabled=true`:
            - then **transfer_to_human_agents** (13 of 13)
          - else:
            - then **transfer_to_human_agents** (5 of 9)
  - else:
    - if `check_network_status:sim card status=active`:
      - if `get_details_by_id:roaming_enabled=false`:
        - if `check_network_status:data roaming enabled=no`:
          - then **toggle_airplane_mode** (6 of 8)
        - else:
          - then **toggle_airplane_mode** (17 of 20)
      - else:
        - if `check_network_status:mobile data enabled=no`:
          - if `called get_details_by_id`:
            - then **toggle_airplane_mode** (8 of 8)
          - else:
            - then **toggle_airplane_mode** (8 of 10)
        - else:
          - then **toggle_airplane_mode** (25 of 25)
    - else:
      - if `check_network_status:mobile data enabled=no`:
        - then **toggle_airplane_mode** (6 of 9)
      - else:
        - if `called get_details_by_id`:
          - if `get_details_by_id:activated=true`:
            - then **toggle_airplane_mode** (5 of 5)
          - else:
            - then **toggle_airplane_mode** (6 of 10)
        - else:
          - then **toggle_airplane_mode** (7 of 8)

### After `toggle_airplane_mode` (404 decisions, 2 deep)

- if `toggle_airplane_mode:status bar=📵 no signal`:
  - if `called reboot_device`:
    - then **check_status_bar** (12 of 13)
  - else:
    - then **check_sim_status** (98 of 161)
- else:
  - if `toggle_airplane_mode:status bar=📱 data enabled`:
    - then **run_speed_test** (26 of 118)
  - else:
    - then **toggle_data** (59 of 112)

### After `set_network_mode_preference` (395 decisions, 4 deep)

- if `ticket: an`:
  - if `called run_speed_test`:
    - if `called get_details_by_id`:
      - then **run_speed_test** (35 of 35)
    - else:
      - then **run_speed_test** (5 of 6)
  - else:
    - if `set_network_mode_preference:status bar=📱 data enabled`:
      - if `check_network_status:airplane mode=on`:
        - then **check_apn_settings** (6 of 21)
      - else:
        - then **can_send_mms** (15 of 37)
    - else:
      - if `called can_send_mms`:
        - then **toggle_data** (4 of 5)
      - else:
        - then **toggle_data** (5 of 5)
- else:
  - if `set_network_mode_preference:status bar=🔽 data saver`:
    - if `called check_network_status`:
      - then **run_speed_test** (12 of 12)
    - else:
      - if `called toggle_roaming`:
        - then **check_data_restriction_status** (5 of 6)
      - else:
        - then **run_speed_test** (3 of 6)
  - else:
    - if `check_network_status:airplane mode=on`:
      - if `get_details_by_id:name=premium plan`:
        - then **run_speed_test** (11 of 13)
      - else:
        - then **run_speed_test** (37 of 40)
    - else:
      - if `toggle_roaming:status bar=🔒 vpn connected`:
        - then **run_speed_test** (47 of 50)
      - else:
        - then **run_speed_test** (156 of 159)

### After `toggle_roaming` (341 decisions, 5 deep)

- if `toggle_roaming:status bar=📱 data enabled`:
  - if `toggle_roaming:status bar=📶¹ poor`:
    - if `called get_details_by_id`:
      - if `toggle_roaming:status bar=🔽 data saver`:
        - if `called enable_roaming`:
          - then **run_speed_test** (4 of 8)
        - else:
          - then **enable_roaming{"customer_id": "C1001", "line_id": "L1002"}** (8 of 16)
      - else:
        - if `called get_data_usage`:
          - then **run_speed_test** (10 of 16)
        - else:
          - then **set_network_mode_preference{"mode": "4g_5g_preferred"}** (18 of 59)
    - else:
      - if `called get_customer_by_phone`:
        - if `toggle_roaming:status bar=🔒 vpn connected`:
          - then **get_details_by_id{"id": "L1001"}** (6 of 6)
        - else:
          - then **get_details_by_id{"id": "L1001"}** (4 of 10)
      - else:
        - if `toggle_airplane_mode:status bar=📵 no signal`:
          - then **run_speed_test** (11 of 11)
        - else:
          - then **run_speed_test** (5 of 6)
  - else:
    - if `ticket: an`:
      - if `called check_apn_settings`:
        - if `called run_speed_test`:
          - then **can_send_mms** (3 of 6)
        - else:
          - then **can_send_mms** (7 of 13)
      - else:
        - if `reseat_sim_card:status bar=📱 data enabled`:
          - then **can_send_mms** (3 of 9)
        - else:
          - then **toggle_roaming** (1 of 5)
    - else:
      - if `toggle_roaming:says data roaming is now off.`:
        - then **toggle_roaming** (6 of 7)
      - else:
        - if `run_speed_test:speed test failed=no connection.`:
          - then **run_speed_test** (81 of 87)
        - else:
          - then **run_speed_test** (14 of 21)
- else:
  - if `toggle_roaming:status bar=✈️ airplane mode`:
    - if `toggle_roaming:says data roaming is now off.`:
      - if `toggle_roaming:status bar=🔒 vpn connected`:
        - then **toggle_airplane_mode** (4 of 5)
      - else:
        - then **toggle_airplane_mode** (4 of 8)
    - else:
      - if `toggle_roaming:status bar=🔒 vpn connected`:
        - then **check_status_bar** (7 of 8)
      - else:
        - then **check_status_bar** (5 of 7)
  - else:
    - if `toggle_roaming:says data roaming is now off.`:
      - then **toggle_roaming** (4 of 5)
    - else:
      - if `ticket: an`:
        - then **toggle_data** (5 of 9)
      - else:
        - if `check_network_status:airplane mode=off`:
          - then **toggle_data** (4 of 7)
        - else:
          - then **toggle_data** (10 of 12)

### After `check_sim_status` (326 decisions, 4 deep)

- if `check_sim_status:says no sim card detected in the phone.`:
  - if `called toggle_airplane_mode`:
    - then **reseat_sim_card** (106 of 106)
  - else:
    - if `check_network_status:airplane mode=off`:
      - then **reseat_sim_card** (27 of 27)
    - else:
      - then **reseat_sim_card** (6 of 8)
- else:
  - if `check_sim_status:says the sim card is locked with a pin code.`:
    - if `get_details_by_id:roaming_enabled=true`:
      - then **transfer_to_human_agents** (10 of 11)
    - else:
      - then **transfer_to_human_agents** (50 of 50)
  - else:
    - if `called set_network_mode_preference`:
      - if `called get_data_usage`:
        - then **reset_apn_settings** (9 of 19)
      - else:
        - then **check_apn_settings** (26 of 31)
    - else:
      - if `reseat_sim_card:status bar=📵 no signal`:
        - then **check_status_bar** (16 of 25)
      - else:
        - then **reset_apn_settings** (20 of 49)

### After `enable_roaming` (310 decisions, 5 deep)

- if `called check_network_status`:
  - if `run_speed_test:speed test failed=no connection.`:
    - if `check_network_status:mobile data enabled=no`:
      - if `toggle_data:status bar=📶¹ poor`:
        - if `called toggle_roaming`:
          - then **run_speed_test** (19 of 22)
        - else:
          - then **run_speed_test** (5 of 7)
      - else:
        - if `called toggle_data`:
          - then **get_data_usage{"customer_id": "C1001", "line_id": "L1002"}** (2 of 5)
        - else:
          - then **toggle_data** (5 of 5)
    - else:
      - if `get_details_by_id:status=active`:
        - if `check_network_status:cellular signal=poor`:
          - then **run_speed_test** (5 of 7)
        - else:
          - then **run_speed_test** (7 of 16)
      - else:
        - if `called refuel_data`:
          - then **run_speed_test** (8 of 8)
        - else:
          - then **run_speed_test** (5 of 8)
  - else:
    - if `ticket: an`:
      - if `called toggle_airplane_mode`:
        - then **run_speed_test** (2 of 6)
      - else:
        - if `check_network_status:cellular signal=poor`:
          - then **check_network_mode_preference** (3 of 8)
        - else:
          - then **can_send_mms** (13 of 20)
    - else:
      - if `check_network_status:mobile data enabled=no`:
        - if `called toggle_data`:
          - then **run_speed_test** (8 of 24)
        - else:
          - then **toggle_data** (9 of 11)
      - else:
        - if `check_network_status:cellular signal=excellent`:
          - then **run_speed_test** (2 of 8)
        - else:
          - then **set_network_mode_preference{"mode": "4g_5g_preferred"}** (8 of 28)
- else:
  - if `get_details_by_id:roaming_enabled=false`:
    - if `ticket: an`:
      - if `called toggle_roaming`:
        - then **can_send_mms** (5 of 5)
      - else:
        - then **toggle_roaming** (7 of 14)
    - else:
      - if `called check_status_bar`:
        - then **toggle_roaming** (3 of 9)
      - else:
        - then **check_network_status** (9 of 13)
  - else:
    - if `get_data_usage:data_used_gb=15.1`:
      - then **refuel_data{"customer_id": "C1001", "gb_amount": 2, "line_id": "L1002"}** (11 of 17)
    - else:
      - if `ticket: an`:
        - then **toggle_roaming** (11 of 15)
      - else:
        - if `get_details_by_id:activated=true`:
          - then **toggle_roaming** (6 of 7)
        - else:
          - then **toggle_roaming** (36 of 47)

### After `get_data_usage` (280 decisions, 3 deep)

- if `get_data_usage:data_used_gb=15.1`:
  - if `toggle_airplane_mode:status bar=📵 data disabled`:
    - if `called run_speed_test`:
      - then **refuel_data{"customer_id": "C1001", "gb_amount": 2, "line_id": "L1002"}** (31 of 43)
    - else:
      - then **refuel_data{"customer_id": "C1001", "gb_amount": 2, "line_id": "L1002"}** (6 of 19)
  - else:
    - if `called enable_roaming`:
      - then **refuel_data{"customer_id": "C1001", "gb_amount": 2, "line_id": "L1002"}** (46 of 53)
    - else:
      - then **refuel_data{"customer_id": "C1001", "gb_amount": 2, "line_id": "L1002"}** (87 of 113)
- else:
  - if `get_details_by_id:status=active`:
    - if `called toggle_roaming`:
      - then **check_network_mode_preference** (2 of 7)
    - else:
      - then **run_speed_test** (5 of 11)
  - else:
    - if `called check_sim_status`:
      - then **check_network_status** (7 of 12)
    - else:
      - then **check_status_bar** (12 of 22)

### After `check_network_mode_preference` (280 decisions, 1 deep)

- if `called set_network_mode_preference`:
  - then **check_wifi_calling_status** (14 of 15)
- else:
  - then **set_network_mode_preference{"mode": "4g_5g_preferred"}** (214 of 265)

### After `refuel_data` (262 decisions, 0 deep)

- then **run_speed_test** (202 of 262)

### After `toggle_data` (253 decisions, 6 deep)

- if `toggle_data:status bar=🔽 data saver`:
  - if `called check_network_status`:
    - if `called enable_roaming`:
      - if `check_status_bar:status bar=✈️ airplane mode`:
        - then **run_speed_test** (5 of 6)
      - else:
        - if `toggle_airplane_mode:status bar=🔒 vpn connected`:
          - then **toggle_data_saver_mode** (3 of 6)
        - else:
          - then **check_data_restriction_status** (3 of 6)
    - else:
      - if `get_details_by_id:roaming_enabled=false`:
        - if `called get_data_usage`:
          - then **enable_roaming{"customer_id": "C1001", "line_id": "L1002"}** (3 of 5)
        - else:
          - then **get_details_by_id{"id": "L1002"}** (4 of 9)
      - else:
        - if `called get_details_by_id`:
          - then **run_speed_test** (4 of 7)
        - else:
          - then **get_details_by_id{"id": "L1001"}** (3 of 7)
  - else:
    - if `called toggle_airplane_mode`:
      - if `toggle_airplane_mode:status bar=🔒 vpn connected`:
        - if `called toggle_roaming`:
          - then **check_data_restriction_status** (4 of 7)
        - else:
          - then **toggle_data_saver_mode** (7 of 8)
      - else:
        - then **toggle_data_saver_mode** (3 of 6)
    - else:
      - if `called enable_roaming`:
        - then **check_data_restriction_status** (5 of 6)
      - else:
        - then **check_data_restriction_status** (3 of 5)
- else:
  - if `toggle_data:status bar=📱 data enabled`:
    - if `toggle_data:status bar=📶¹ poor`:
      - if `check_network_status:sim card status=active`:
        - if `get_details_by_id:roaming_enabled=true`:
          - then **toggle_roaming** (2 of 6)
        - else:
          - if `called get_details_by_id`:
            - then **enable_roaming{"customer_id": "C1001", "line_id": "L1002"}** (6 of 18)
          - else:
            - then **run_speed_test** (8 of 11)
      - else:
        - if `made enable_roaming{"customer_id": "C1001", "line_id": "L1002"}`:
          - then **set_network_mode_preference{"mode": "4g_5g_preferred"}** (8 of 8)
        - else:
          - if `called can_send_mms`:
            - then **run_speed_test** (4 of 14)
          - else:
            - then **check_network_mode_preference** (11 of 17)
    - else:
      - if `ticket: an`:
        - if `check_network_status:data roaming enabled=no`:
          - then **toggle_roaming** (4 of 9)
        - else:
          - if `get_details_by_id:roaming_enabled=false`:
            - then **can_send_mms** (4 of 8)
          - else:
            - then **can_send_mms** (6 of 14)
      - else:
        - if `ticket: abroad`:
          - if `called check_status_bar`:
            - then **run_speed_test** (7 of 7)
          - else:
            - then **toggle_roaming** (4 of 11)
        - else:
          - if `get_details_by_id:roaming_enabled=true`:
            - then **run_speed_test** (6 of 7)
          - else:
            - then **run_speed_test** (27 of 27)
  - else:
    - if `check_network_status:cellular connection=connected`:
      - then **toggle_data** (11 of 11)
    - else:
      - then **toggle_data** (3 of 7)

### After `reboot_device` (241 decisions, 4 deep)

- if `ticket: an`:
  - if `run_speed_test:speed test failed=no connection.`:
    - then **run_speed_test** (7 of 9)
  - else:
    - if `reboot_device:status bar=📶⁴ excellent`:
      - if `called can_send_mms`:
        - then **can_send_mms** (74 of 83)
      - else:
        - then **check_wifi_calling_status** (5 of 8)
    - else:
      - then **check_status_bar** (2 of 7)
- else:
  - if `ticket: all`:
    - if `reboot_device:status bar=📶⁴ excellent`:
      - if `get_details_by_id:roaming_enabled=true`:
        - then **stop** (10 of 15)
      - else:
        - then **stop** (21 of 21)
    - else:
      - if `get_details_by_id:status=paid`:
        - then **check_status_bar** (25 of 26)
      - else:
        - then **check_status_bar** (3 of 7)
  - else:
    - if `reboot_device:status bar=📶⁴ excellent`:
      - if `run_speed_test:speed test failed=no connection.`:
        - then **run_speed_test** (42 of 42)
      - else:
        - then **run_speed_test** (4 of 5)
    - else:
      - if `called check_network_status`:
        - then **get_data_usage{"customer_id": "C1001", "line_id": "L1002"}** (3 of 8)
      - else:
        - then **run_speed_test** (6 of 10)

### After `reseat_sim_card` (236 decisions, 3 deep)

- if `reseat_sim_card:status bar=📵 no signal`:
  - if `get_details_by_id:status=paid`:
    - if `check_sim_status:says no sim card detected in the phone.`:
      - then **check_sim_status** (22 of 23)
    - else:
      - then **check_sim_status** (7 of 11)
  - else:
    - if `called check_network_status`:
      - then **check_sim_status** (31 of 39)
    - else:
      - then **check_sim_status** (7 of 17)
- else:
  - if `ticket: an`:
    - if `check_network_status:mobile data enabled=no`:
      - then **toggle_data** (17 of 41)
    - else:
      - then **can_send_mms** (21 of 66)
  - else:
    - if `toggle_airplane_mode:status bar=📵 no signal`:
      - then **stop** (15 of 24)
    - else:
      - then **reboot_device** (8 of 15)

### After `check_apn_settings` (226 decisions, 5 deep)

- if `check_apn_settings:mmsc url (for picture messages)=http://mms.carrier.com/mms/wapenc`:
  - if `ticket: an`:
    - if `called check_wifi_calling_status`:
      - if `reboot_device:status bar=📶⁴ excellent`:
        - if `check_network_status:sim card status=active`:
          - then **check_app_permissions{"app_name": "messaging"}** (4 of 8)
        - else:
          - then **check_wifi_calling_status** (1 of 5)
      - else:
        - if `reseat_sim_card:status bar=📱 data enabled`:
          - then **can_send_mms** (3 of 12)
        - else:
          - then **check_installed_apps** (3 of 5)
    - else:
      - if `called set_network_mode_preference`:
        - if `check_status_bar:status bar=📵 data disabled`:
          - then **check_installed_apps** (3 of 7)
        - else:
          - then **can_send_mms** (12 of 24)
      - else:
        - if `check_network_status:data roaming enabled=yes`:
          - then **check_wifi_calling_status** (6 of 16)
        - else:
          - then **check_network_status** (3 of 11)
  - else:
    - if `toggle_airplane_mode:status bar=📶¹ poor`:
      - if `toggle_data:status bar=🔒 vpn connected`:
        - then **check_data_restriction_status** (2 of 9)
      - else:
        - if `toggle_roaming:says data roaming is now off.`:
          - then **reseat_sim_card** (6 of 10)
        - else:
          - then **check_network_status** (6 of 19)
    - else:
      - if `run_speed_test:speed test failed=no connection.`:
        - if `called check_sim_status`:
          - then **check_data_restriction_status** (9 of 11)
        - else:
          - then **reset_apn_settings** (3 of 8)
      - else:
        - then **reset_apn_settings** (8 of 9)
- else:
  - if `run_speed_test:speed test failed=no connection.`:
    - then **reset_apn_settings** (5 of 9)
  - else:
    - if `check_wifi_calling_status:says wi-fi calling is currently turned off.`:
      - if `called enable_roaming`:
        - then **reset_apn_settings** (3 of 6)
      - else:
        - if `called check_installed_apps`:
          - then **reset_apn_settings** (6 of 6)
        - else:
          - then **reset_apn_settings** (4 of 5)
    - else:
      - if `called get_details_by_id`:
        - if `called toggle_roaming`:
          - then **reset_apn_settings** (25 of 25)
        - else:
          - then **reset_apn_settings** (11 of 13)
      - else:
        - then **reset_apn_settings** (6 of 8)

### After `check_app_permissions` (202 decisions, 4 deep)

- if `check_app_permissions:app 'messaging' has permission for=sms, storage, phone.`:
  - if `called reboot_device`:
    - if `check_network_status:data roaming enabled=no`:
      - then **can_send_mms** (3 of 7)
    - else:
      - then **check_network_mode_preference** (2 of 7)
  - else:
    - if `called check_apn_settings`:
      - then **reset_apn_settings** (5 of 9)
    - else:
      - if `called toggle_roaming`:
        - then **check_apn_settings** (5 of 5)
      - else:
        - then **check_apn_settings** (4 of 5)
- else:
  - if `check_app_permissions:says app 'messaging' not found on this phone.`:
    - if `called check_apn_settings`:
      - if `called reset_apn_settings`:
        - then **check_installed_apps** (4 of 6)
      - else:
        - then **check_installed_apps** (8 of 9)
    - else:
      - if `called set_network_mode_preference`:
        - then **check_apn_settings** (4 of 6)
      - else:
        - then **check_installed_apps** (6 of 7)
  - else:
    - if `check_app_permissions:app 'messaging' has permission for=sms, phone.`:
      - if `get_details_by_id:activated=true`:
        - then **grant_app_permission{"app_name": "messaging", "permission": "storage"}** (9 of 19)
      - else:
        - then **grant_app_permission{"app_name": "messaging", "permission": "storage"}** (36 of 42)
    - else:
      - if `check_app_permissions:app 'messaging' has permission for=phone.`:
        - then **grant_app_permission{"app_name": "messaging", "permission": "storage"}** (24 of 46)
      - else:
        - then **grant_app_permission{"app_name": "messaging", "permission": "sms"}** (21 of 34)

### After `check_data_restriction_status` (201 decisions, 2 deep)

- if `check_data_restriction_status:says data saver mode is off.`:
  - if `called set_network_mode_preference`:
    - then **check_vpn_status** (32 of 48)
  - else:
    - then **check_network_mode_preference** (38 of 68)
- else:
  - if `called run_speed_test`:
    - then **toggle_data_saver_mode** (52 of 54)
  - else:
    - then **toggle_data_saver_mode** (22 of 31)

### After `reset_apn_settings` (183 decisions, 0 deep)

- then **reboot_device** (163 of 183)

### After `toggle_data_saver_mode` (162 decisions, 3 deep)

- if `toggle_data_saver_mode:status bar=📶¹ poor`:
  - if `called run_speed_test`:
    - if `run_speed_test:speed test failed=no connection.`:
      - then **check_network_mode_preference** (10 of 16)
    - else:
      - then **run_speed_test** (21 of 42)
  - else:
    - if `check_status_bar:status bar=🔒 vpn connected`:
      - then **check_network_mode_preference** (14 of 21)
    - else:
      - then **check_network_mode_preference** (14 of 29)
- else:
  - if `called run_speed_test`:
    - if `check_status_bar:status bar=📶⁴ excellent`:
      - then **run_speed_test** (3 of 6)
    - else:
      - then **run_speed_test** (30 of 32)
  - else:
    - if `toggle_roaming:status bar=🔒 vpn connected`:
      - then **check_vpn_status** (4 of 5)
    - else:
      - then **run_speed_test** (7 of 11)

### After `disconnect_vpn` (155 decisions, 0 deep)

- then **run_speed_test** (130 of 155)

### After `check_wifi_calling_status` (140 decisions, 2 deep)

- if `check_wifi_calling_status:says wi-fi calling is currently turned off.`:
  - if `called check_apn_settings`:
    - then **check_app_permissions{"app_name": "messaging"}** (14 of 43)
  - else:
    - then **check_installed_apps** (11 of 25)
- else:
  - if `run_speed_test:speed test failed=no connection.`:
    - then **toggle_wifi_calling** (4 of 6)
  - else:
    - then **toggle_wifi_calling** (64 of 66)

### After `check_vpn_status` (131 decisions, 0 deep)

- then **disconnect_vpn** (101 of 131)

### After `get_bills_for_customer` (131 decisions, 2 deep)

- if `called get_details_by_id`:
  - if `called check_payment_request`:
    - then **resume_line{"customer_id": "C1001", "line_id": "L1002"}** (8 of 12)
  - else:
    - then **send_payment_request{"bill_id": "B1234321", "customer_id": "C1001"}** (63 of 94)
- else:
  - then **get_details_by_id{"id": "L1002"}** (20 of 25)

### After `grant_app_permission` (123 decisions, 2 deep)

- if `check_app_permissions:app 'messaging' has permission for=phone.`:
  - if `made grant_app_permission{"app_name": "messaging", "permission": "sms"}`:
    - then **can_send_mms** (28 of 33)
  - else:
    - then **grant_app_permission{"app_name": "messaging", "permission": "sms"}** (8 of 11)
- else:
  - if `called check_status_bar`:
    - then **can_send_mms** (52 of 54)
  - else:
    - then **can_send_mms** (18 of 25)

### After `transfer_to_human_agents` (111 decisions, 0 deep)

- then **stop** (109 of 111)

### After `send_payment_request` (105 decisions, 0 deep)

- then **check_payment_request** (102 of 105)

### After `check_payment_request` (103 decisions, 0 deep)

- then **make_payment** (101 of 103)

### After `make_payment` (102 decisions, 1 deep)

- if `made send_payment_request{"bill_id": "B1234321", "customer_id": "C1001"}`:
  - then **get_details_by_id{"id": "B1234321"}** (37 of 75)
- else:
  - then **get_details_by_id{"id": "B1002"}** (26 of 27)

### After `check_installed_apps` (83 decisions, 0 deep)

- then **check_app_permissions{"app_name": "messaging"}** (74 of 83)

### After `toggle_wifi_calling` (75 decisions, 0 deep)

- then **can_send_mms** (59 of 75)

### After `resume_line` (71 decisions, 0 deep)

- then **check_status_bar** (48 of 71)

### After `set_apn_settings` (19 decisions, 0 deep)

- then **can_send_mms** (10 of 19)

### After `check_app_status` (17 decisions, 1 deep)

- if `check_app_status:says - storage`:
  - then **grant_app_permission{"app_name": "messaging", "permission": "sms"}** (8 of 10)
- else:
  - then **grant_app_permission{"app_name": "messaging", "permission": "storage"}** (6 of 7)

### After `resume_line!` (3 decisions, 0 deep)

- then **reset_apn_settings** (1 of 3)

### After `toggle_wifi` (1 decisions, 0 deep)

- then **run_speed_test** (1 of 1)

# Matching descriptions to records

At each write that picks records out of earlier tool results (the items of an order, the variant an item becomes, a payment method, a reservation) and has at least two candidates, a System-One model is shown what the customer said and the candidates, but not the agent's pick, and asked which one the customer means. Both picks are scored against the task's expected actions. Used as a check, the model flags a write where its pick differs from the agent's.

## retail

| Choice | Choices | Agent right | Model right | Model differs | ... agent wrong there | Agent wrong, model right | Unanswered |
|---|---|---|---|---|---|---|---|
| Items of the order | 1403 | 1312 (93.5%) | 1143 (81.5%) | 227 (16.2%) | 37 (16.3%) | 21 (23.1%) | 0 |
| New variant | 1302 | 1185 (91.0%) | 1087 (83.5%) | 143 (11.0%) | 27 (18.9%) | 18 (15.4%) | 0 |
| Payment method | 506 | 420 (83.0%) | 408 (80.6%) | 26 (5.1%) | 7 (26.9%) | 7 (8.1%) | 0 |
| All | 3211 | 2917 (90.8%) | 2638 (82.2%) | 396 (12.3%) | 71 (17.9%) | 46 (15.6%) | 0 |

The model right, the agent wrong (up to 8):

- task 13, payment method: agent ["credit_card_3124723"], model ["paypal_9497703"], expected ["paypal_9497703"]; the customer last said "Yes, please go ahead and process the return for those items. Thank you!"
- task 12, payment method: agent ["credit_card_3124723"], model ["paypal_9497703"], expected ["paypal_9497703"]; the customer last said "Yes, that’s correct. Please go ahead and process the return and refund to my PayPal."
- task 98, items of the order: agent ["7758198585"], model ["4068787148", "7758198585"], expected ["4068787148", "7758198585"]; the customer last said "Yes, that sounds good. Please go ahead and process the exchange for the bicycle."
- task 98, items of the order: agent ["4068787148"], model ["4068787148", "7758198585"], expected ["4068787148", "7758198585"]; the customer last said "Yes, please go ahead and process the exchange for the jigsaw puzzle as well. That option works for me."
- task 98, items of the order: agent ["7758198585"], model ["4068787148", "7758198585"], expected ["4068787148", "7758198585"]; the customer last said "Yes, I confirm the exchange for the bicycle as you summarized. Let’s move on to the next item, please."
- task 98, items of the order: agent ["4068787148"], model ["4068787148", "7758198585"], expected ["4068787148", "7758198585"]; the customer last said "Yes, I confirm the exchange for the jigsaw puzzle as you described. Let’s continue with the camera exchange next."
- task 12, payment method: agent ["credit_card_3124723"], model ["paypal_9497703"], expected ["paypal_9497703"]; the customer last said "I want to keep just the things related to gaming, and return everything else from both orders. Please refund me through PayPal."
- task 20, items of the order: agent ["9791469541"], model ["1340995114", "1763705424", "2366567022", "9791469541"], expected ["1340995114", "1763705424", "2366567022", "9791469541"]; the customer last said "Yes, please go ahead and make the change. Use my gift card for the difference."

The agent right, the model wrong (up to 8):

- task 6, items of the order: agent ["8384507844"], model ["8384507844", "8538875209"], expected ["8384507844"]; the customer last said "Yes, I confirm. Please proceed with the exchange for the desk lamp."
- task 11, payment method: agent ["paypal_9497703"], model ["credit_card_3124723"], expected ["paypal_9497703"]; the customer last said "Yes. Just do it."
- task 21, items of the order: agent ["1340995114", "9791469541"], model [], expected ["1340995114", "9791469541"]; the customer last said "Yes, please go ahead and make both changes. And just to confirm, after everything is updated, my gift card balance will be $44.08, right?"
- task 20, new variant: agent ["4579334072"], model ["2439754078"], expected ["4579334072"]; the customer last said "Yes, please upgrade all of them to the premium versions you listed. I’d like to use my gift card to pay the difference. If that’s not possible, PayPal is fine."
- task 23, items of the order: agent ["6301799585"], model ["6301799585", "7082455361"], expected ["6301799585"]; the customer last said "For the helmet, I’d like to exchange it for a medium size, red, high ventilation type.  For the luggage set, I want to exchange it for a two-piece, black set wi"
- task 23, new variant: agent ["7082455361"], model ["9724317332"], expected ["7082455361"]; the customer last said "For the helmet, I’d like to exchange it for a medium size, red, high ventilation type.  For the luggage set, I want to exchange it for a two-piece, black set wi"
- task 35, items of the order: agent ["1684786391"], model ["1684786391", "3778566150"], expected ["1684786391"]; the customer last said "Finally, some actual options. I want the 13-inch i5 in silver—so that’s the one with 1TB SSD, right? Go ahead and change my order to that.  And for the speakers"
- task 36, items of the order: agent ["3799046073", "6117189161", "7453605304"], model ["3799046073", "6117189161", "7453605304", "9851293632", "9879255677"], expected ["3799046073", "6117189161", "7453605304"]; the customer last said "Yes, please go ahead and make those changes. Use the same card on file."

## airline

| Choice | Choices | Agent right | Model right | Model differs | ... agent wrong there | Agent wrong, model right | Unanswered |
|---|---|---|---|---|---|---|---|
| Payment method | 295 | 244 (82.7%) | 232 (78.6%) | 30 (10.2%) | 12 (40.0%) | 6 (11.8%) | 0 |
| Reservation | 472 | 415 (87.9%) | 377 (79.9%) | 185 (39.2%) | 31 (16.8%) | 11 (19.3%) | 0 |
| All | 767 | 659 (85.9%) | 609 (79.4%) | 215 (28.0%) | 43 (20.0%) | 17 (15.7%) | 0 |

The model right, the agent wrong (up to 8):

- task 33, payment method: agent ["gift_card_1646646"], model ["gift_card_6941833"], expected ["gift_card_6941833"]; the customer last said "Let’s go with option 1: please upgrade only my outbound flight (HAT072) to business class for the $152, and keep the return flight in economy. I’ll use my 3 fre"
- task 44, payment method: agent ["gift_card_5094406"], model ["credit_card_4196779"], expected ["credit_card_4196779"]; the customer last said "Thank you for breaking down the costs and availability. I’d like to go ahead and upgrade all three reservations (NM1VX1, KC18K6, and H8Q05L) to business class. "
- task 7, reservation: agent ["7WPL39"], model ["XEHM4B"], expected ["XEHM4B"]; the customer last said "Both of those are basic economy. I want to upgrade both to economy. Can you do that?"
- task 7, reservation: agent ["3EMQJ6"], model ["XEHM4B"], expected ["XEHM4B"]; the customer last said "Both of those are basic economy. I want to upgrade both to economy. Can you do that?"
- task 39, reservation: agent ["4XGCCM"], model ["8C8K4E"], expected ["8C8K4E", "LU15PA", "MSJ4OA"]; the customer last said "Oui, please go ahead and cancel those three business class flights: 8C8K4E, LU15PA, and 4XGCCM. I understand for the others, c’est pas possible. Merci for expla"
- task 42, reservation: agent ["SE9KEL"], model ["FDZ0T5"], expected ["FDZ0T5", "HSR97W"]; the customer last said "That looks perfect—please go ahead and cancel those four reservations. Thank you for helping me clear this up!"
- task 42, reservation: agent ["PUNERT"], model ["FDZ0T5"], expected ["FDZ0T5", "HSR97W"]; the customer last said "That looks perfect—please go ahead and cancel those four reservations. Thank you for helping me clear this up!"
- task 42, reservation: agent ["SE9KEL"], model ["FDZ0T5"], expected ["FDZ0T5", "HSR97W"]; the customer last said "Yes, that’s correct—please go ahead and cancel the LAX to BOS and JFK to PHL flights on May 17, and the ORD to SFO flight on May 22. The reason for all these ca"

The agent right, the model wrong (up to 8):

- task 9, reservation: agent ["NQNU5R"], model ["IFOYYZ"], expected ["NQNU5R"]; the customer last said "Yes, please go ahead and cancel both IFOYYZ and NQNU5R. Since there are no nonstop flights available for M20IZO, I’ll keep that reservation as it is. Please use"
- task 39, reservation: agent ["8C8K4E"], model ["4XGCCM"], expected ["8C8K4E", "LU15PA", "MSJ4OA"]; the customer last said "Merci for checking! I just want to cancel all my flights so that someone else can have my seat if they need it. I understand that I might not get refunds for so"
- task 39, reservation: agent ["LU15PA"], model ["4XGCCM"], expected ["8C8K4E", "LU15PA", "MSJ4OA"]; the customer last said "Merci for checking! I just want to cancel all my flights so that someone else can have my seat if they need it. I understand that I might not get refunds for so"
- task 39, reservation: agent ["MSJ4OA"], model ["4XGCCM"], expected ["8C8K4E", "LU15PA", "MSJ4OA"]; the customer last said "Merci for checking! I just want to cancel all my flights so that someone else can have my seat if they need it. I understand that I might not get refunds for so"
- task 44, payment method: agent ["credit_card_4196779"], model ["gift_card_5094406"], expected ["credit_card_4196779"]; the customer last said "Thank you for the breakdown! I’d like to go ahead and upgrade all of the eligible flights to business class. Please proceed with the upgrades."
- task 9, reservation: agent ["NQNU5R"], model ["IFOYYZ"], expected ["NQNU5R"]; the customer last said "Thank you for checking the details. The reason I need to cancel both reservations is due to a change in my travel plans—I will no longer be able to make those t"
- task 39, reservation: agent ["MSJ4OA"], model ["4XGCCM"], expected ["8C8K4E", "LU15PA", "MSJ4OA"]; the customer last said "Merci for checking. I just want to cancel all of them, même if I do not get a refund. I want to free up my seats for someone else who might need them. Please go"
- task 39, reservation: agent ["LU15PA"], model ["4XGCCM"], expected ["8C8K4E", "LU15PA", "MSJ4OA"]; the customer last said "Merci for checking. I just want to cancel all of them, même if I do not get a refund. I want to free up my seats for someone else who might need them. Please go"

